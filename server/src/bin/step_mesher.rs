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
        .or_else(|| std::env::var("MITTENS_STEP_DEFLECTION").ok().and_then(|v| v.parse().ok()))
        .unwrap_or_else(|| default_deflection_for_path(&input));

    tracing::info!("Meshing STEP: {} (deflection={})", input.display(), deflection);

    #[cfg(not(feature = "occt-support"))]
    {
        let _ = deflection;
        return Err(anyhow!("STEP support disabled at compile time; rebuild with feature 'occt-support'"));
    }

    #[cfg(feature = "occt-support")]
    {
        use opencascade::primitives::Shape;

        let shape = Shape::read_step(&input)
            .with_context(|| format!("failed to read STEP {}", input.display()))?;

        // OCCT meshing can throw via FFI in some cases; keep this in a helper process so
        // the parent server doesn't die. Still, try to surface panics cleanly.
        let mesh = std::panic::catch_unwind(|| shape.mesh_with_deflection(deflection))
            .map_err(|_| anyhow!("STEP meshing panicked"))?;

        let mut out = Vec::<u8>::new();
        write_mesh_packet(
            &mut out,
            &mesh.vertices,
            &mesh.normals,
            &mesh.indices,
        )?;

        // OCCT can write diagnostics to stdout from C++ in some builds. If we mix text with
        // binary, the renderer will fail to parse. Prefer writing to a file when requested.
        if let Some(p) = out_path {
            let mut f = std::fs::File::create(&p)
                .with_context(|| format!("failed to create output file {}", p.display()))?;
            f.write_all(&out)?;
            f.flush()?;
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
    out.reserve(
        8
            + (num_vertices as usize) * 3 * 4 * 3
            + (num_indices as usize) * 4,
    );
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
