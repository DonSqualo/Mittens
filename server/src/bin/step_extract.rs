use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use opencascade_sys::ffi;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PickMode {
    /// Pick the solid with smallest volume/area (rough proxy for a thin plate).
    Thin,
    /// Pick the solid with largest volume.
    Largest,
}

#[derive(Debug, Parser)]
#[command(name = "step_extract")]
#[command(about = "List solids in a STEP file and export a selected solid to its own STEP file")]
struct Cli {
    /// Input STEP/STP file
    input: PathBuf,

    /// List solids with basic properties (volume/area/center of mass)
    #[arg(long)]
    list: bool,

    /// Print groups of solids that repeat exactly N times (based on a geometry signature).
    /// This is useful for finding repeated sub-assemblies (e.g. 5 toolheads).
    #[arg(long)]
    repeated: Option<usize>,

    /// Export the Nth solid (0-based) into a standalone STEP
    #[arg(long)]
    export_index: Option<usize>,

    /// Auto-pick a solid by heuristic and export it
    #[arg(long, value_enum)]
    pick: Option<PickMode>,

    /// Output STEP path (required for --export-index/--pick)
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone)]
struct SolidInfo {
    idx: usize,
    volume: f64,
    area: f64,
    com: (f64, f64, f64),
    faces: i32,
    edges: i32,
    vertices: i32,
}

fn read_step_shape(path: &Path) -> Result<cxx::UniquePtr<ffi::TopoDS_Shape>> {
    let mut reader = ffi::STEPControl_Reader_ctor();
    let status = ffi::read_step(reader.pin_mut(), path.to_string_lossy().to_string());
    if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
        bail!("failed to read STEP (status={:?})", status);
    }
    reader.pin_mut().TransferRoots(&ffi::Message_ProgressRange_ctor());
    Ok(ffi::one_shape(&reader))
}

fn solid_info_for(shape: &ffi::TopoDS_Shape, idx: usize) -> Result<SolidInfo> {
    let mut vol_props = ffi::GProp_GProps_ctor();
    ffi::BRepGProp_VolumeProperties(shape, vol_props.pin_mut());
    let volume = vol_props.Mass();
    let com_p = ffi::GProp_GProps_CentreOfMass(&vol_props);
    let com_p = com_p.as_ref().ok_or_else(|| anyhow!("failed to get center of mass"))?;
    let com = (com_p.X(), com_p.Y(), com_p.Z());

    let mut area_props = ffi::GProp_GProps_ctor();
    ffi::BRepGProp_SurfaceProperties(shape, area_props.pin_mut());
    let area = area_props.Mass();

    let faces = {
        let mut m = ffi::new_indexed_map_of_shape();
        ffi::map_shapes(shape, ffi::TopAbs_ShapeEnum::TopAbs_FACE, m.pin_mut());
        m.Extent()
    };
    let edges = {
        let mut m = ffi::new_indexed_map_of_shape();
        ffi::map_shapes(shape, ffi::TopAbs_ShapeEnum::TopAbs_EDGE, m.pin_mut());
        m.Extent()
    };
    let vertices = {
        let mut m = ffi::new_indexed_map_of_shape();
        ffi::map_shapes(shape, ffi::TopAbs_ShapeEnum::TopAbs_VERTEX, m.pin_mut());
        m.Extent()
    };

    Ok(SolidInfo {
        idx,
        volume,
        area,
        com,
        faces,
        edges,
        vertices,
    })
}

fn q(v: f64) -> i64 {
    // Quantize to 1e-6 in model units; keeps identical solids together even if minor fp drift.
    (v * 1_000_000.0).round() as i64
}

fn signature(info: &SolidInfo) -> (i64, i64, i32, i32, i32) {
    (q(info.volume), q(info.area), info.faces, info.edges, info.vertices)
}

fn list_and_pick(
    shape: &ffi::TopoDS_Shape,
    do_list: bool,
    pick: Option<PickMode>,
    repeated: Option<usize>,
) -> Result<(usize, Option<usize>)> {
    let mut explorer = ffi::TopExp_Explorer_ctor(shape, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);
    let mut count = 0usize;

    let mut best_idx: Option<usize> = None;
    let mut best_metric: f64 = f64::INFINITY;
    let mut best_volume: f64 = 0.0;

    let mut sig_groups: std::collections::HashMap<(i64, i64, i32, i32, i32), Vec<SolidInfo>> =
        std::collections::HashMap::new();

    if do_list {
        println!("# idx\tvolume\tarea\tfaces\tedges\tverts\tvol/area\tcom(x,y,z)");
    }

    while explorer.More() {
        let cur = explorer.Current();
        let info = solid_info_for(cur, count)?;
        let ratio = if info.area > 1e-12 { info.volume / info.area } else { f64::NAN };

        if do_list {
            println!(
                "{}\t{:.6e}\t{:.6e}\t{}\t{}\t{}\t{:.6e}\t({:.3},{:.3},{:.3})",
                info.idx,
                info.volume,
                info.area,
                info.faces,
                info.edges,
                info.vertices,
                ratio,
                info.com.0,
                info.com.1,
                info.com.2
            );
        }

        if repeated.is_some() {
            sig_groups
                .entry(signature(&info))
                .or_default()
                .push(info.clone());
        }

        if let Some(mode) = pick {
            match mode {
                PickMode::Largest => {
                    if best_idx.is_none() || info.volume > best_volume {
                        best_volume = info.volume;
                        best_idx = Some(info.idx);
                    }
                }
                PickMode::Thin => {
                    if info.area > 1e-12 && info.volume > 0.0 {
                        let metric = info.volume / info.area;
                        if metric < best_metric {
                            best_metric = metric;
                            best_idx = Some(info.idx);
                        }
                    }
                }
            }
        }

        count += 1;
        explorer.pin_mut().Next();
    }

    if let Some(n) = repeated {
        let mut groups: Vec<(usize, (i64, i64, i32, i32, i32), Vec<SolidInfo>)> = sig_groups
            .into_iter()
            .filter_map(|(sig, infos)| {
                if infos.len() == n {
                    Some((infos.len(), sig, infos))
                } else {
                    None
                }
            })
            .collect();
        groups.sort_by(|a, b| b.0.cmp(&a.0));

        println!("# repeated={} groups={}", n, groups.len());
        for (_cnt, sig, mut infos) in groups {
            infos.sort_by(|a, b| a.idx.cmp(&b.idx));
            let idxs: Vec<String> = infos.iter().map(|i| i.idx.to_string()).collect();
            let sample = &infos[0];
            println!(
                "count={}\tvol={:.6e}\tarea={:.6e}\tfaces={}\tedges={}\tverts={}\tidxs=[{}]\tsig=({},{},{},{},{})",
                n,
                sample.volume,
                sample.area,
                sample.faces,
                sample.edges,
                sample.vertices,
                idxs.join(","),
                sig.0, sig.1, sig.2, sig.3, sig.4
            );
        }
    }

    Ok((count, best_idx))
}

fn export_solid_by_index(root: &ffi::TopoDS_Shape, idx: usize, out_path: &Path) -> Result<()> {
    let mut explorer = ffi::TopExp_Explorer_ctor(root, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);
    let mut cur_idx = 0usize;
    while explorer.More() {
        if cur_idx == idx {
            let cur = explorer.Current();
            // Translate the solid so its center-of-mass is near the origin. This makes the
            // viewer's auto-frame and orbit controls behave predictably.
            let info = solid_info_for(cur, cur_idx)?;
            let mut trsf = ffi::new_transform();
            let v = ffi::new_vec(-info.com.0, -info.com.1, -info.com.2);
            trsf.pin_mut().set_translation_vec(&v);

            let mut xform = ffi::BRepBuilderAPI_Transform_ctor(cur, &trsf, false);
            let progress = ffi::Message_ProgressRange_ctor();
            xform.pin_mut().Build(&progress);
            if !xform.IsDone() {
                bail!("OCCT transform failed while centering solid {}", idx);
            }

            write_step(xform.pin_mut().Shape(), out_path)?;
            return Ok(());
        }
        cur_idx += 1;
        explorer.pin_mut().Next();
    }
    bail!("solid index {} out of range (solids={})", idx, cur_idx)
}

fn write_step(shape: &ffi::TopoDS_Shape, out_path: &Path) -> Result<()> {
    let mut writer = ffi::STEPControl_Writer_ctor();
    let status = ffi::transfer_shape(writer.pin_mut(), shape);
    if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
        bail!("STEP transfer failed (status={:?})", status);
    }
    let status = ffi::write_step(writer.pin_mut(), out_path.to_string_lossy().to_string());
    if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
        bail!("STEP write failed (status={:?})", status);
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    eprintln!("[step_extract] loading {}", cli.input.display());
    let shape = read_step_shape(&cli.input)
        .with_context(|| format!("failed to load STEP {}", cli.input.display()))?;
    let shape_ref = shape.as_ref().ok_or_else(|| anyhow!("STEP reader produced null shape"))?;
    eprintln!("[step_extract] loaded STEP, scanning solids...");

    let (count, picked) = list_and_pick(shape_ref, cli.list, cli.pick, cli.repeated)?;
    if cli.list {
        println!("# solids={}", count);
    }

    let export_idx = match (cli.export_index, picked) {
        (Some(i), _) => Some(i),
        (None, Some(i)) => Some(i),
        _ => None,
    };

    if let Some(i) = export_idx {
        let out_path = cli.out.as_ref().ok_or_else(|| anyhow!("--out is required for export"))?;
        export_solid_by_index(shape_ref, i, out_path)
            .with_context(|| format!("failed to write {}", out_path.display()))?;
        eprintln!("wrote {}", out_path.display());
    }

    Ok(())
}
