use anyhow::{anyhow, Result};
use mlua::{Lua, Table, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "../src/conformance.rs"]
mod conformance;
#[path = "../src/export.rs"]
mod export;
#[path = "../src/thread_primitives.rs"]
mod thread_primitives;
#[path = "../src/geometry.rs"]
mod geometry;
#[path = "../src/ir.rs"]
mod ir;

struct ExportRegressionCase {
    script: PathBuf,
    baseline_dir: PathBuf,
    export_files: Vec<&'static str>,
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

fn unique_temp_dir(prefix: &str) -> Result<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow!("clock error: {e}"))?
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{}_{}", prefix, nonce));
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn run_export_regression(case: &ExportRegressionCase) -> Result<()> {
    let lua = Lua::new();
    configure_lua_package_path(&lua)?;

    let content = fs::read_to_string(&case.script)?;
    let result: Value = lua.load(&content).eval()?;
    let table = result
        .as_table()
        .ok_or_else(|| anyhow!("{} did not return scene table", case.script.display()))?;

    let out_dir = unique_temp_dir("mittens_stl_regression")?;
    export::process_exports_from_table(&lua, table, &out_dir);

    let oracle = conformance::OracleConfig {
        surface_samples: 2500,
        volume_samples: 8000,
        boundary_band_mm: 0.05,
        max_surface_p95_mm: 0.05,
        max_inside_disagreement_rate: 0.001,
    };

    for filename in &case.export_files {
        let generated = out_dir.join(filename);
        let baseline = case.baseline_dir.join(filename);

        if !generated.exists() {
            return Err(anyhow!(
                "Generated STL missing: {} (from {})",
                generated.display(),
                case.script.display()
            ));
        }
        if !baseline.exists() {
            return Err(anyhow!("Baseline STL missing: {}", baseline.display()));
        }

        let generated_mesh = conformance::load_binary_stl(&generated)?;
        let baseline_mesh = conformance::load_binary_stl(&baseline)?;
        let report = conformance::compare_meshes(&generated_mesh, &baseline_mesh, oracle)?;

        if !report.pass {
            return Err(anyhow!(
                "Regression for {} failed: {:?}",
                filename,
                report.failed_checks
            ));
        }
    }

    let _ = fs::remove_dir_all(&out_dir);
    Ok(())
}

#[test]
fn project_stl_regression() -> Result<()> {
    let projects_root = PathBuf::from("/home/heim/projects");
    let mode = std::env::var("MITTENS_PROJECT_STL_REGRESSION").ok();
    if mode.as_deref() == Some("0") {
        eprintln!("Skipping project STL regression (MITTENS_PROJECT_STL_REGRESSION=0)");
        return Ok(());
    }
    if mode.as_deref() != Some("1") && !projects_root.exists() {
        eprintln!(
            "Skipping project STL regression (projects root missing: {})",
            projects_root.display()
        );
        return Ok(());
    }

    let cases = vec![
        ExportRegressionCase {
            script: projects_root.join("pure-acoustics/pure_acoustics.lua"),
            baseline_dir: projects_root.join("pure-acoustics"),
            export_files: vec!["icd.stl", "lid.stl", "holder_adapter.stl"],
        },
        ExportRegressionCase {
            script: projects_root.join("helmholtz/helmholtz_coil.lua"),
            baseline_dir: projects_root.join("helmholtz"),
            export_files: vec!["helmholtz_scaffold.stl", "coupling_coil_adapter.stl"],
        },
    ];

    for case in &cases {
        if !case.script.exists() {
            return Err(anyhow!("Missing script: {}", case.script.display()));
        }
        run_export_regression(case)?;
    }

    Ok(())
}
