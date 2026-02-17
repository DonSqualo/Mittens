use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "step_mesher")]
#[command(about = "Mesh a STEP file and emit Mittens binary mesh packet")]
struct Cli {
    /// Input STEP/STP file
    input: PathBuf,

    /// OCCT meshing deflection (millimeters-ish; larger = coarser)
    #[arg(long)]
    deflection: Option<f64>,

    /// Output path for binary mesh packet (recommended). If omitted, writes to stdout.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> Result<()> {
    // Keep logs off stdout since stdout is binary mesh payload.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_level(true)
        .init();

    let cli = Cli::parse();
    let input = cli.input;
    let out_path = cli.out;

    let deflection = cli
        .deflection
        .or_else(|| {
            std::env::var("MITTENS_STEP_DEFLECTION")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or_else(|| default_deflection_for_path(&input));

    tracing::info!(
        "Meshing STEP: {} (deflection={})",
        input.display(),
        deflection
    );

    #[cfg(not(feature = "occt-support"))]
    {
        let _ = deflection;
        return Err(anyhow!(
            "STEP support disabled at compile time; rebuild with feature 'occt-support'"
        ));
    }

    #[cfg(feature = "occt-support")]
    {
        use opencascade::primitives::Shape;
        use opencascade_sys::ffi;

        let disable_fixshape = std::env::var("MITTENS_STEP_DISABLE_FIXSHAPE")
            .ok()
            .map(|v| {
                let v = v.trim().to_ascii_lowercase();
                v == "1" || v == "true" || v == "yes" || v == "on"
            })
            .unwrap_or(false);
        if disable_fixshape {
            // OCCT STEP import applies ShapeFix by default, which can crash on some large/complex
            // assemblies. Disabling it is a pragmatic "load something" fallback.
            let _ = ffi::interface_static_set_cval("FromSTEP.exec.op".to_string(), "".to_string());
        }

        // For large assemblies we want real progress. OCCT doesn't surface a nice percent for
        // whole-shape meshing, so we optionally mesh solid-by-solid and emit done/total.
        let prefer_solid_progress = std::env::var("MITTENS_STEP_PROGRESS_SOLIDS")
            .ok()
            .map(|v| {
                let v = v.trim().to_ascii_lowercase();
                v == "1" || v == "true" || v == "yes" || v == "on"
            })
            .unwrap_or_else(|| {
                let sz = std::fs::metadata(&input).ok().map(|m| m.len()).unwrap_or(0);
                sz > 50 * 1024 * 1024
            });

        eprintln!("MITTENS_PROGRESS {{\"phase\":\"read_step\",\"done\":0,\"total\":1}}");

        let shape = {
            let mut reader = ffi::STEPControl_Reader_ctor();
            let status = ffi::read_step(reader.pin_mut(), input.to_string_lossy().to_string());
            if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
                return Err(anyhow!("failed to read STEP (status={:?})", status));
            }
            reader
                .pin_mut()
                .TransferRoots(&ffi::Message_ProgressRange_ctor());
            let one = ffi::one_shape(&reader);
            let shape_ref = one
                .as_ref()
                .ok_or_else(|| anyhow!("STEP reader produced null shape"))?;
            let owned = ffi::TopoDS_Shape_to_owned(shape_ref);
            Shape::from_topods_owned(owned)
        };

        eprintln!("MITTENS_PROGRESS {{\"phase\":\"read_step\",\"done\":1,\"total\":1}}");

        let topods = shape
            .as_topods()
            .ok_or_else(|| anyhow!("null STEP shape"))?;

        let mesh = if !prefer_solid_progress {
            // OCCT meshing can throw via FFI in some cases; keep this in a helper process so
            // the parent server doesn't die. Still, try to surface panics cleanly.
            std::panic::catch_unwind(|| shape.mesh_with_deflection(deflection))
                .map_err(|_| anyhow!("STEP meshing panicked"))?
        } else {
            // Two-pass: count solids first for a real done/total progress signal.
            let mut explorer =
                ffi::TopExp_Explorer_ctor(topods, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);
            let mut total_solids: u32 = 0;
            while explorer.More() {
                total_solids += 1;
                explorer.pin_mut().Next();
            }
            if total_solids == 0 {
                return Err(anyhow!("STEP contains no solids"));
            }

            eprintln!(
                "MITTENS_PROGRESS {{\"phase\":\"mesh_solids\",\"done\":0,\"total\":{}}}",
                total_solids
            );

            let mut out_vertices: Vec<glam::DVec3> = Vec::new();
            let mut out_normals: Vec<glam::DVec3> = Vec::new();
            let mut out_indices: Vec<usize> = Vec::new();

            let mut explorer2 =
                ffi::TopExp_Explorer_ctor(topods, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);
            let mut done: u32 = 0;
            while explorer2.More() {
                let cur = explorer2.Current();
                let owned = ffi::TopoDS_Shape_to_owned(cur);
                let solid = Shape::from_topods_owned(owned);

                let m = std::panic::catch_unwind(|| solid.mesh_with_deflection(deflection))
                    .map_err(|_| anyhow!("STEP meshing panicked (solid)"))?;

                let base = out_vertices.len();
                out_vertices.extend_from_slice(&m.vertices);
                out_normals.extend_from_slice(&m.normals);
                out_indices.extend(m.indices.into_iter().map(|i| base + i));

                done += 1;
                if done == 1 || done == total_solids || done % 5 == 0 {
                    eprintln!(
                        "MITTENS_PROGRESS {{\"phase\":\"mesh_solids\",\"done\":{},\"total\":{}}}",
                        done, total_solids
                    );
                }

                explorer2.pin_mut().Next();
            }

            opencascade::mesh::Mesh {
                vertices: out_vertices,
                uvs: Vec::new(),
                normals: out_normals,
                indices: out_indices,
            }
        };

        let mut out = Vec::<u8>::new();
        write_mesh_packet(&mut out, &mesh.vertices, &mesh.normals, &mesh.indices)?;

        // OCCT can write diagnostics to stdout from C++ in some builds. If we mix text with
        // binary, the renderer will fail to parse. Prefer writing to a file when requested.
        if let Some(p) = out_path {
            eprintln!("MITTENS_PROGRESS {{\"phase\":\"write_mesh\",\"done\":0,\"total\":1}}");
            let mut f = std::fs::File::create(&p)
                .with_context(|| format!("failed to create output file {}", p.display()))?;
            f.write_all(&out)?;
            f.flush()?;
            eprintln!("MITTENS_PROGRESS {{\"phase\":\"write_mesh\",\"done\":1,\"total\":1}}");
        } else {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&out)?;
            stdout.flush()?;
        }
        Ok(())
    }
}

fn default_deflection_for_path(path: &PathBuf) -> f64 {
    let sz = std::fs::metadata(path).ok().map(|m| m.len()).unwrap_or(0);
    if sz > 150 * 1024 * 1024 {
        // Very large assemblies: default to a coarse tessellation to avoid OOM and keep
        // first-launch interactive. Override via MITTENS_STEP_DEFLECTION / --deflection.
        30.0
    } else if sz > 50 * 1024 * 1024 {
        10.0
    } else {
        0.05
    }
}

fn write_mesh_packet(
    out: &mut Vec<u8>,
    vertices: &[glam::DVec3],
    normals: &[glam::DVec3],
    indices: &[usize],
) -> Result<()> {
    let num_vertices = vertices.len() as u32;
    let num_indices = indices.len() as u32;

    if num_vertices == 0 || num_indices == 0 {
        return Err(anyhow!("mesher produced empty mesh"));
    }

    // Packet format must match `MeshData::to_binary`:
    // [u32 num_vertices][u32 num_indices]
    // [positions f32 * 3][normals f32 * 3][colors f32 * 3][indices u32]
    out.reserve(8 + (num_vertices as usize) * 3 * 4 * 3 + (num_indices as usize) * 4);
    out.extend_from_slice(&num_vertices.to_le_bytes());
    out.extend_from_slice(&num_indices.to_le_bytes());

    for p in vertices {
        out.extend_from_slice(&(p.x as f32).to_le_bytes());
        out.extend_from_slice(&(p.y as f32).to_le_bytes());
        out.extend_from_slice(&(p.z as f32).to_le_bytes());
    }

    if normals.len() == vertices.len() {
        for n in normals {
            out.extend_from_slice(&(n.x as f32).to_le_bytes());
            out.extend_from_slice(&(n.y as f32).to_le_bytes());
            out.extend_from_slice(&(n.z as f32).to_le_bytes());
        }
    } else {
        // Fallback: zero normals (renderer uses flat shading anyway).
        for _ in 0..vertices.len() {
            out.extend_from_slice(&0f32.to_le_bytes());
            out.extend_from_slice(&0f32.to_le_bytes());
            out.extend_from_slice(&0f32.to_le_bytes());
        }
    }

    // Default white vertex color.
    for _ in 0..vertices.len() {
        out.extend_from_slice(&1f32.to_le_bytes());
        out.extend_from_slice(&1f32.to_le_bytes());
        out.extend_from_slice(&1f32.to_le_bytes());
    }

    for &idx in indices {
        out.extend_from_slice(&(idx as u32).to_le_bytes());
    }

    Ok(())
}
