use anyhow::{anyhow, Result};
use clap::Parser;
use mlua::{Lua, Table, Value};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "../conformance.rs"]
mod conformance;
#[path = "../thread_primitives.rs"]
mod thread_primitives;
#[path = "../geometry.rs"]
mod geometry;
#[path = "../ir.rs"]
mod ir;

#[derive(Parser, Debug)]
#[command(name = "ir_subagent")]
#[command(about = "Canonical geometry IR + mesh validation quick loop")]
struct Cli {
    #[arg(long, default_value = "../examples/multiphysics/pure_acoustics.lua")]
    file: PathBuf,
    #[arg(long, default_value_t = 64)]
    circular_segments: u32,
    #[arg(long, default_value = "target/ir_subagent")]
    out_dir: PathBuf,
    #[arg(long)]
    candidate_stl: Option<PathBuf>,
    #[arg(long, default_value_t = 2000)]
    surface_samples: usize,
    #[arg(long, default_value_t = 6000)]
    volume_samples: usize,
    #[arg(long, default_value_t = 0.05)]
    boundary_band_mm: f32,
    #[arg(long, default_value_t = 0.15)]
    max_surface_p95_mm: f32,
    #[arg(long, default_value_t = 0.01)]
    max_inside_disagreement_rate: f64,
    #[arg(long, default_value_t = false)]
    emit_baseline_stl: bool,
}

#[derive(Serialize)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Serialize)]
struct MeshSummary {
    vertices: usize,
    triangles: usize,
    removed_degenerate_triangles: usize,
    bounds: Option<Bounds>,
}

#[derive(Serialize)]
struct ValidationSummary {
    valid: bool,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct Summary {
    file: String,
    circular_segments: u32,
    object_count: usize,
    scene_hash: String,
    mesh: MeshSummary,
    validation: ValidationSummary,
    conformance: Option<conformance::OracleReport>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let lua = Lua::new();

    configure_lua_package_path(&lua)?;

    let content = fs::read_to_string(&cli.file)?;
    let result_value: Value = lua.load(&content).eval()?;

    let scene_table = result_value
        .as_table()
        .ok_or_else(|| anyhow!("Lua script must return scene table"))?;
    let object_count = scene_table
        .get::<_, Table>("objects")
        .map_err(|_| anyhow!("Scene table missing 'objects'"))?
        .len()? as usize;

    let circular_segments = resolve_circular_segments(&scene_table, cli.circular_segments);

    let canonical_scene = ir::scene_from_lua_value(&result_value)?;
    let scene_hash = ir::scene_hash(&canonical_scene)?;

    let mesh = geometry::generate_mesh_from_ir_scene(&canonical_scene, circular_segments)?;
    let validation = geometry::validate_mesh(&mesh);

    let mut cleaned_mesh = geometry::MeshData {
        positions: mesh.positions.clone(),
        normals: mesh.normals.clone(),
        colors: mesh.colors.clone(),
        indices: mesh.indices.clone(),
    };
    let removed = geometry::remove_degenerate_triangles(&mut cleaned_mesh);
    let baseline_stl_path = cli.out_dir.join("baseline_manifold.stl");

    fs::create_dir_all(&cli.out_dir)?;

    if cli.emit_baseline_stl {
        conformance::write_binary_stl(&mesh, &baseline_stl_path)?;
    }

    let oracle_config = conformance::OracleConfig {
        surface_samples: cli.surface_samples,
        volume_samples: cli.volume_samples,
        boundary_band_mm: cli.boundary_band_mm,
        max_surface_p95_mm: cli.max_surface_p95_mm,
        max_inside_disagreement_rate: cli.max_inside_disagreement_rate,
    };

    let conformance_report = if let Some(candidate_path) = &cli.candidate_stl {
        let candidate_mesh = conformance::load_binary_stl(candidate_path)?;
        Some(conformance::compare_meshes(
            &mesh,
            &candidate_mesh,
            oracle_config,
        )?)
    } else {
        None
    };

    let summary = Summary {
        file: cli.file.display().to_string(),
        circular_segments,
        object_count,
        scene_hash,
        mesh: MeshSummary {
            vertices: mesh.positions.len() / 3,
            triangles: mesh.indices.len() / 3,
            removed_degenerate_triangles: removed,
            bounds: compute_bounds(&mesh),
        },
        validation: ValidationSummary {
            valid: validation.valid,
            warnings: validation.warnings,
        },
        conformance: conformance_report.clone(),
    };

    fs::write(
        cli.out_dir.join("canonical_ir.json"),
        serde_json::to_vec_pretty(&canonical_scene)?,
    )?;
    fs::write(
        cli.out_dir.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    if let Some(report) = &conformance_report {
        fs::write(
            cli.out_dir.join("conformance.json"),
            serde_json::to_vec_pretty(report)?,
        )?;
    }

    println!("ir_subagent completed");
    println!("file: {}", summary.file);
    println!("scene_hash: {}", summary.scene_hash);
    println!(
        "mesh: {} vertices, {} triangles, removed_degenerate={}",
        summary.mesh.vertices, summary.mesh.triangles, summary.mesh.removed_degenerate_triangles
    );
    if cli.emit_baseline_stl {
        println!("baseline_stl: {}", baseline_stl_path.display());
    }
    if let Some(candidate_path) = &cli.candidate_stl {
        if let Some(report) = &conformance_report {
            println!(
                "conformance: candidate={} pass={} symmetric_p95_mm={:.6} disagreement_rate_outside_band={:.6}",
                candidate_path.display(),
                report.pass,
                report.symmetric_p95_mm,
                report.inside_outside.disagreement_rate_outside_boundary_band
            );
        }
    }
    println!("artifacts: {}", cli.out_dir.display());

    Ok(())
}

fn configure_lua_package_path(lua: &Lua) -> Result<()> {
    let package: Table = lua.globals().get("package")?;
    let existing_path: String = package.get("path").unwrap_or_default();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .ok_or_else(|| anyhow!("failed to resolve repo root from CARGO_MANIFEST_DIR"))?;

    let stdlib_glob = repo_root.join("?.lua");
    let stdlib_init_glob = repo_root.join("?/init.lua");
    let new_path = format!(
        "{};{};{}",
        stdlib_glob.display(),
        stdlib_init_glob.display(),
        existing_path
    );

    package.set("path", new_path)?;
    Ok(())
}

fn resolve_circular_segments(scene_table: &Table, default_segments: u32) -> u32 {
    if let Ok(view) = scene_table.get::<_, Table>("view") {
        return view
            .get::<_, u32>("circular_segments")
            .unwrap_or(default_segments);
    }
    default_segments
}

fn compute_bounds(mesh: &geometry::MeshData) -> Option<Bounds> {
    if mesh.positions.is_empty() {
        return None;
    }

    let mut min = [f32::MAX, f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN, f32::MIN];

    for i in 0..(mesh.positions.len() / 3) {
        let x = mesh.positions[i * 3];
        let y = mesh.positions[i * 3 + 1];
        let z = mesh.positions[i * 3 + 2];

        min[0] = min[0].min(x);
        min[1] = min[1].min(y);
        min[2] = min[2].min(z);

        max[0] = max[0].max(x);
        max[1] = max[1].max(y);
        max[2] = max[2].max(z);
    }

    Some(Bounds { min, max })
}
