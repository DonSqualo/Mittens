use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use opencascade_sys::ffi;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum PickMode {
    /// Pick the solid with smallest volume/area (rough proxy for a thin plate).
    Thin,
    /// Pick the solid with largest volume.
    Largest,
    /// Pick the solid (or repeated group member) with smallest bbox min-Y.
    MinY,
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

    /// When used with `--repeated N --pick min-y`, bundle nearby repeated solids (same instance
    /// position across repeated groups) into a single exported STEP compound.
    ///
    /// This is useful when a "part" is represented as multiple solids (e.g. plate + extrusions).
    #[arg(long)]
    bundle_nearby_mm: Option<f64>,

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
    // Some STEP files can trigger OCCT FixShape crashes. Allow disabling it for this tool as well,
    // matching the step_mesher behavior.
    if std::env::var("MITTENS_STEP_DISABLE_FIXSHAPE")
        .ok()
        .as_deref()
        == Some("1")
    {
        let _ = ffi::interface_static_set_cval("FromSTEP.exec.op".to_string(), "".to_string());
    }

    let mut reader = ffi::STEPControl_Reader_ctor();
    let status = ffi::read_step(reader.pin_mut(), path.to_string_lossy().to_string());
    if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
        bail!("failed to read STEP (status={:?})", status);
    }
    reader
        .pin_mut()
        .TransferRoots(&ffi::Message_ProgressRange_ctor());
    Ok(ffi::one_shape(&reader))
}

fn solid_info_for(shape: &ffi::TopoDS_Shape, idx: usize) -> Result<SolidInfo> {
    let mut vol_props = ffi::GProp_GProps_ctor();
    ffi::BRepGProp_VolumeProperties(shape, vol_props.pin_mut());
    let volume = vol_props.Mass();
    let com_p = ffi::GProp_GProps_CentreOfMass(&vol_props);
    let com_p = com_p
        .as_ref()
        .ok_or_else(|| anyhow!("failed to get center of mass"))?;
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

fn bbox_for(shape: &ffi::TopoDS_Shape) -> Result<Option<(f64, f64, f64, f64, f64, f64)>> {
    let mut b = ffi::Bnd_Box_ctor();
    let mut bpin = b.pin_mut();
    ffi::BRepBndLib_Add(shape, bpin.as_mut());
    let mut xmin = 0.0;
    let mut ymin = 0.0;
    let mut zmin = 0.0;
    let mut xmax = 0.0;
    let mut ymax = 0.0;
    let mut zmax = 0.0;
    let ok = ffi::Bnd_Box_Get(
        b.as_ref().ok_or_else(|| anyhow!("null bbox"))?,
        &mut xmin,
        &mut ymin,
        &mut zmin,
        &mut xmax,
        &mut ymax,
        &mut zmax,
    );
    if ok {
        Ok(Some((xmin, ymin, zmin, xmax, ymax, zmax)))
    } else {
        Ok(None)
    }
}

fn q(v: f64) -> i64 {
    // Quantize to 1e-6 in model units; keeps identical solids together even if minor fp drift.
    (v * 1_000_000.0).round() as i64
}

fn signature(info: &SolidInfo) -> (i64, i64, i32, i32, i32) {
    (
        q(info.volume),
        q(info.area),
        info.faces,
        info.edges,
        info.vertices,
    )
}

fn list_and_pick(
    shape: &ffi::TopoDS_Shape,
    do_list: bool,
    pick: Option<PickMode>,
    repeated: Option<usize>,
    bundle_nearby_mm: Option<f64>,
) -> Result<(usize, Option<usize>, Vec<usize>)> {
    let mut explorer = ffi::TopExp_Explorer_ctor(shape, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);
    let mut count = 0usize;
    let mut infos: Vec<SolidInfo> = Vec::new();

    while explorer.More() {
        let cur = explorer.Current();
        let info = solid_info_for(cur, count)?;
        infos.push(info);
        count += 1;
        explorer.pin_mut().Next();
    }

    if do_list {
        println!("# idx\tvolume\tarea\tfaces\tedges\tverts\tvol/area\tcom(x,y,z)");
        for info in &infos {
            let ratio = if info.area > 1e-12 {
                info.volume / info.area
            } else {
                f64::NAN
            };
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
    }

    let mut best_idx: Option<usize> = None;
    if let Some(mode) = pick {
        match mode {
            PickMode::Largest => {
                best_idx = infos
                    .iter()
                    .max_by(|a, b| {
                        a.volume
                            .partial_cmp(&b.volume)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|i| i.idx);
            }
            PickMode::Thin => {
                best_idx = infos
                    .iter()
                    .filter(|i| i.area > 1e-12 && i.volume > 0.0)
                    .min_by(|a, b| {
                        let ra = a.volume / a.area;
                        let rb = b.volume / b.area;
                        ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|i| i.idx);
            }
            PickMode::MinY => {
                // If the user asked for repeated groups, we'll pick among those (see below).
            }
        }
    }

    // Group repeated solids by signature.
    let mut sig_groups: std::collections::HashMap<(i64, i64, i32, i32, i32), Vec<SolidInfo>> =
        std::collections::HashMap::new();
    if repeated.is_some() {
        for info in &infos {
            sig_groups
                .entry(signature(info))
                .or_default()
                .push(info.clone());
        }
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

        let mut best_repeated_miny: f64 = f64::INFINITY;
        let mut best_repeated_idx: Option<usize> = None;

        // Only compute bboxes for the indices we care about (repeated groups), to avoid
        // touching weird/invalid solids elsewhere in the STEP.
        let mut candidate_idxs: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for (_cnt, _sig, infos) in &groups {
            for i in infos {
                candidate_idxs.insert(i.idx);
            }
        }

        let mut bbox_by_idx: std::collections::HashMap<usize, (f64, f64, f64, f64, f64, f64)> =
            std::collections::HashMap::new();
        let mut explorer2 = ffi::TopExp_Explorer_ctor(shape, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);
        let mut idx2 = 0usize;
        while explorer2.More() {
            if candidate_idxs.contains(&idx2) {
                let cur = explorer2.Current();
                if let Ok(Some(bb)) = bbox_for(cur) {
                    bbox_by_idx.insert(idx2, bb);
                }
            }
            idx2 += 1;
            explorer2.pin_mut().Next();
        }

        for (_cnt, sig, infos) in &groups {
            let mut infos = infos.clone();
            infos.sort_by(|a, b| a.idx.cmp(&b.idx));
            let idxs: Vec<String> = infos.iter().map(|i| i.idx.to_string()).collect();
            let sample = &infos[0];
            let group_miny = infos
                .iter()
                .filter_map(|i| bbox_by_idx.get(&i.idx).map(|b| b.1))
                .fold(f64::INFINITY, |a, b| a.min(b));
            let group_maxy = infos
                .iter()
                .filter_map(|i| bbox_by_idx.get(&i.idx).map(|b| b.4))
                .fold(f64::NEG_INFINITY, |a, b| a.max(b));

            if let Some(mode) = pick {
                if mode == PickMode::MinY && group_miny < best_repeated_miny {
                    best_repeated_miny = group_miny;
                    // Pick the specific instance that reaches most into -Y.
                    best_repeated_idx = infos
                        .iter()
                        .filter_map(|i| bbox_by_idx.get(&i.idx).map(|b| (i.idx, b.1)))
                        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                        .map(|x| x.0);
                }
            }
            println!(
                "count={}\tvol={:.6e}\tarea={:.6e}\tfaces={}\tedges={}\tverts={}\tminy={:.6e}\tmaxy={:.6e}\tidxs=[{}]\tsig=({},{},{},{},{})",
                n,
                sample.volume,
                sample.area,
                sample.faces,
                sample.edges,
                sample.vertices,
                group_miny,
                group_maxy,
                idxs.join(","),
                sig.0, sig.1, sig.2, sig.3, sig.4
            );
        }

        let mut bundle_idxs: Vec<usize> = Vec::new();

        // If the user asked to pick min-Y and also asked for repeated groups,
        // prefer the repeated selection.
        if pick == Some(PickMode::MinY) {
            if let Some(idx) = best_repeated_idx {
                if let Some(bb) = bbox_by_idx.get(&idx) {
                    println!(
                        "# pick=min-y idx={} bbox(xmin={:.6e},ymin={:.6e},zmin={:.6e},xmax={:.6e},ymax={:.6e},zmax={:.6e})",
                        idx, bb.0, bb.1, bb.2, bb.3, bb.4, bb.5
                    );
                } else {
                    println!("# pick=min-y idx={}", idx);
                }
                best_idx = Some(idx);

                // Optional bundling: find other repeated solids that belong to the same instance
                // (same ordinal position within each repeated-`n` group), and are spatially
                // near the picked solid.
                if let (Some(thr), Some(sel_bb)) = (bundle_nearby_mm, bbox_by_idx.get(&idx)) {
                    let sel_center = (
                        (sel_bb.0 + sel_bb.3) * 0.5,
                        (sel_bb.1 + sel_bb.4) * 0.5,
                        (sel_bb.2 + sel_bb.5) * 0.5,
                    );

                    // Determine which "instance slot" (0..n-1) the picked solid is in.
                    let mut picked_slot: Option<usize> = None;
                    for (_cnt, _sig, infos) in &groups {
                        let mut idxs: Vec<usize> = infos.iter().map(|i| i.idx).collect();
                        idxs.sort();
                        if let Some(pos) = idxs.iter().position(|&x| x == idx) {
                            picked_slot = Some(pos);
                            break;
                        }
                    }

                    if let Some(slot) = picked_slot {
                        let mut chosen: Vec<usize> = Vec::new();
                        chosen.push(idx);

                        for (_cnt, _sig, infos) in &groups {
                            let mut idxs: Vec<usize> = infos.iter().map(|i| i.idx).collect();
                            idxs.sort();
                            if slot >= idxs.len() {
                                continue;
                            }
                            let other_idx = idxs[slot];
                            let Some(bb) = bbox_by_idx.get(&other_idx) else {
                                continue;
                            };
                            let center = (
                                (bb.0 + bb.3) * 0.5,
                                (bb.1 + bb.4) * 0.5,
                                (bb.2 + bb.5) * 0.5,
                            );
                            let dx = center.0 - sel_center.0;
                            let dy = center.1 - sel_center.1;
                            let dz = center.2 - sel_center.2;
                            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                            if dist <= thr {
                                chosen.push(other_idx);
                            }
                        }

                        chosen.sort();
                        chosen.dedup();
                        if chosen.len() > 1 {
                            println!(
                                "# bundle_nearby_mm={} slot={} idxs=[{}]",
                                thr,
                                slot,
                                chosen
                                    .iter()
                                    .map(|i| i.to_string())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            );
                            bundle_idxs = chosen;
                        }
                    }
                }
            }
        }
        return Ok((count, best_idx, bundle_idxs));
    }

    Ok((count, best_idx, Vec::new()))
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

fn export_compound_by_indices(
    root: &ffi::TopoDS_Shape,
    idxs: &[usize],
    out_path: &Path,
) -> Result<()> {
    if idxs.is_empty() {
        bail!("no indices provided for compound export");
    }

    let want: std::collections::HashSet<usize> = idxs.iter().copied().collect();

    let mut compound = ffi::TopoDS_Compound_ctor();
    let builder = ffi::BRep_Builder_ctor();
    let builder = ffi::BRep_Builder_upcast_to_topods_builder(&builder);
    builder.MakeCompound(compound.pin_mut());
    let mut compound_shape = ffi::TopoDS_Compound_as_shape(compound);

    let mut explorer = ffi::TopExp_Explorer_ctor(root, ffi::TopAbs_ShapeEnum::TopAbs_SOLID);
    let mut cur_idx = 0usize;
    let mut com_acc = (0.0f64, 0.0f64, 0.0f64);
    let mut vol_acc = 0.0f64;
    let mut added = 0usize;

    while explorer.More() {
        if want.contains(&cur_idx) {
            let cur = explorer.Current();
            builder.Add(compound_shape.pin_mut(), cur);

            if let Ok(info) = solid_info_for(cur, cur_idx) {
                if info.volume.is_finite() && info.volume > 0.0 {
                    com_acc.0 += info.com.0 * info.volume;
                    com_acc.1 += info.com.1 * info.volume;
                    com_acc.2 += info.com.2 * info.volume;
                    vol_acc += info.volume;
                }
            }
            added += 1;
        }
        cur_idx += 1;
        explorer.pin_mut().Next();
    }

    if added == 0 {
        bail!(
            "none of the requested indices were found (max_idx={})",
            cur_idx
        );
    }

    let com = if vol_acc > 0.0 {
        (
            com_acc.0 / vol_acc,
            com_acc.1 / vol_acc,
            com_acc.2 / vol_acc,
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    let mut trsf = ffi::new_transform();
    let v = ffi::new_vec(-com.0, -com.1, -com.2);
    trsf.pin_mut().set_translation_vec(&v);

    let compound_ref = compound_shape
        .as_ref()
        .ok_or_else(|| anyhow!("compound shape is null"))?;
    let mut xform = ffi::BRepBuilderAPI_Transform_ctor(compound_ref, &trsf, false);
    let progress = ffi::Message_ProgressRange_ctor();
    xform.pin_mut().Build(&progress);
    if !xform.IsDone() {
        bail!("OCCT transform failed while centering compound");
    }

    write_step(xform.pin_mut().Shape(), out_path)?;
    Ok(())
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
    let shape_ref = shape
        .as_ref()
        .ok_or_else(|| anyhow!("STEP reader produced null shape"))?;
    eprintln!("[step_extract] loaded STEP, scanning solids...");

    let (count, picked, bundle) = list_and_pick(
        shape_ref,
        cli.list,
        cli.pick,
        cli.repeated,
        cli.bundle_nearby_mm,
    )?;
    if cli.list {
        println!("# solids={}", count);
    }

    let export_idx = match (cli.export_index, picked) {
        (Some(i), _) => Some(i),
        (None, Some(i)) => Some(i),
        _ => None,
    };

    if let Some(i) = export_idx {
        let out_path = cli
            .out
            .as_ref()
            .ok_or_else(|| anyhow!("--out is required for export"))?;
        // If the user asked for bundling and we computed a non-trivial bundle, export a compound.
        if cli.export_index.is_none() && bundle.len() > 1 && bundle.contains(&i) {
            export_compound_by_indices(shape_ref, &bundle, out_path)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
        } else {
            export_solid_by_index(shape_ref, i, out_path)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
        }
        eprintln!("wrote {}", out_path.display());
    }

    Ok(())
}
