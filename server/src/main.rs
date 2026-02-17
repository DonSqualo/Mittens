//! Mittens Server - Manifold CSG
//! - File watcher
//! - Lua parser
//! - Manifold mesh generation
//! - WebSocket binary mesh streaming

use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use std::{io::Write, net::SocketAddr, path::PathBuf, sync::Arc, thread, time::{Duration, SystemTime, UNIX_EPOCH}};
use tokio::sync::{broadcast, mpsc, RwLock};
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

mod acoustic;
mod cad_io;
mod circuit;
mod export;
mod field;
mod geometry;
mod ir;
mod occt_ir;
mod nanovna;
mod thread_primitives;

type Packet = Arc<[u8]>;

struct AppState {
    mesh_tx: broadcast::Sender<Packet>,
    labels_tx: broadcast::Sender<String>,
    exports_tx: broadcast::Sender<String>,
    current_view: RwLock<Option<Packet>>,
    current_mesh: RwLock<Option<Packet>>,
    current_field: RwLock<Option<Packet>>,
    current_circuit: RwLock<Option<Packet>>,
    current_nanovna: RwLock<Option<Packet>>,
    current_labels: RwLock<Option<String>>,
    current_exports: RwLock<Vec<String>>,
    current_scene: RwLock<Option<Arc<ir::SceneIr>>>,
    watched_file: String,
    git_branch: String,
    port: u16,
}

fn packet_from_vec(v: Vec<u8>) -> Packet {
    Arc::from(v.into_boxed_slice())
}

fn step_mesher_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("MITTENS_STEP_MESHER_PATH") {
        return Ok(PathBuf::from(p));
    }

    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("failed to resolve current_exe parent dir"))?;

    // Common dev layout: server runs from `target/debug`, but the STEP mesher is much more
    // stable (and faster) in release mode. Prefer it automatically if present.
    if dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s == "debug")
        .unwrap_or(false)
    {
        if let Some(parent) = dir.parent() {
            let release = parent.join("release").join("step_mesher");
            if release.exists() {
                return Ok(release);
            }
        }
    }

    Ok(dir.join("step_mesher"))
}

fn default_step_deflection_for_path(path: &PathBuf) -> f64 {
    let sz = std::fs::metadata(path).ok().map(|m| m.len()).unwrap_or(0);
    if sz > 150 * 1024 * 1024 {
        30.0
    } else if sz > 50 * 1024 * 1024 {
        10.0
    } else {
        0.05
    }
}

fn step_mesh_cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(dir).join("mittens").join("step_mesh");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("mittens")
            .join("step_mesh");
    }
    std::env::temp_dir().join("mittens_step_mesh_cache")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn step_mesh_cache_key(input: &PathBuf, deflection: f64) -> Result<String> {
    let canon = std::fs::canonicalize(input).unwrap_or_else(|_| input.clone());
    let meta = std::fs::metadata(&canon)
        .with_context(|| format!("failed to stat STEP {}", canon.display()))?;
    let len = meta.len();
    let mtime_ns: u128 = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    // Stable key across runs (no randomized hasher).
    let s = format!(
        "{}|{}|{}|{:.9}",
        canon.display(),
        len,
        mtime_ns,
        deflection
    );
    Ok(format!("{:016x}", fnv1a64(s.as_bytes())))
}

fn step_mesh_cache_path(input: &PathBuf, deflection: f64) -> Result<PathBuf> {
    let key = step_mesh_cache_key(input, deflection)?;
    Ok(step_mesh_cache_dir().join(format!("stepmesh_{}.bin", key)))
}

fn parse_mesh_stats(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 8 {
        return None;
    }
    let nv = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let ni = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    Some((nv, ni))
}

fn mesh_step_cached_or_run(input: &PathBuf, deflection: f64) -> Result<(Vec<u8>, bool)> {
    let cache_path = step_mesh_cache_path(input, deflection)?;
    if let Ok(bytes) = std::fs::read(&cache_path) {
        // Minimal sanity check: [u32 num_vertices][u32 num_indices]
        if parse_mesh_stats(&bytes).is_some() {
            return Ok((bytes, true));
        }
    }

    let bytes = mesh_step_via_subprocess(input, deflection)?;

    if parse_mesh_stats(&bytes).is_some() {
        let dir = step_mesh_cache_dir();
        let _ = std::fs::create_dir_all(&dir);
        let tmp = dir.join(format!(
            ".tmp_stepmesh_{}_{}.bin",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        if std::fs::write(&tmp, &bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &cache_path);
            let _ = std::fs::remove_file(&tmp);
        }
    }

    Ok((bytes, false))
}

fn mesh_step_via_subprocess(input: &PathBuf, deflection: f64) -> Result<Vec<u8>> {
    let mesher = step_mesher_path()?;

    // Keep meshing isolated and bounded; OCCT can use huge amounts of memory on large assemblies.
    // Use `prlimit` (if available) to cap address space for the mesher process so the parent
    // server stays alive and the system doesn't get dragged into OOM-killer territory.
    // `prlimit --as=...` has been observed to trigger OCCT instability on some systems.
    // Keep it opt-in; users can set MITTENS_STEP_MESHER_MAX_AS_MB if they need a hard cap.
    let max_as_mb: u64 = std::env::var("MITTENS_STEP_MESHER_MAX_AS_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let run_once = |disable_fixshape: bool| -> Result<Vec<u8>> {
        let mut cmd = if max_as_mb == 0 {
            std::process::Command::new(&mesher)
        } else {
            let max_as_bytes = max_as_mb
                .saturating_mul(1024)
                .saturating_mul(1024);
            let mut c = std::process::Command::new("prlimit");
            c.arg(format!("--as={}", max_as_bytes))
                .arg("--")
                .arg(&mesher);
            c
        };

        // STEP meshing only needs OCCT. If the parent process has Manifold's bundled libc++/libc++abi
        // early in LD_LIBRARY_PATH, it can destabilize OCCT at runtime. Prefer an OCCT-only path for
        // the mesher subprocess when possible.
        if let Ok(ld) = std::env::var("LD_LIBRARY_PATH") {
            let occt_only: Vec<&str> = ld
                .split(':')
                .filter(|p| p.contains("/occt") || p.contains("occt"))
                .collect();
            if !occt_only.is_empty() {
                cmd.env("LD_LIBRARY_PATH", occt_only.join(":"));
            }
        }

        if disable_fixshape {
            cmd.env("MITTENS_STEP_DISABLE_FIXSHAPE", "1");
        }

        let tmp_dir = std::env::var_os("MITTENS_STEP_MESHER_TMP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir());
        let nonce = format!(
            "mittens_step_{}_{}_{}_{}.bin",
            std::process::id(),
            if disable_fixshape { "nofix" } else { "fix" },
            deflection.to_string().replace('.', "_"),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let out_path = tmp_dir.join(nonce);

        let output = cmd
            .arg("--deflection")
            .arg(deflection.to_string())
            .arg("--out")
            .arg(&out_path)
            .arg(input)
            // Some OCCT builds can emit diagnostics on stdout; avoid corrupting binary output.
            .stdout(std::process::Stdio::null())
            .output()
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to spawn step mesher {}: {}",
                    mesher.display(),
                    e
                )
            })?;

        if !output.status.success() {
            let _ = std::fs::remove_file(&out_path);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = if stderr.len() > 16 * 1024 {
                format!("{}...[truncated]", &stderr[..16 * 1024])
            } else {
                stderr.to_string()
            };
            return Err(anyhow::anyhow!(
                "step mesher failed (status={}) stderr={}",
                output.status,
                stderr
            ));
        }

        let bytes = std::fs::read(&out_path)
            .with_context(|| format!("failed to read step mesher output {}", out_path.display()))?;
        let _ = std::fs::remove_file(&out_path);
        Ok(bytes)
    };

    // Retry strategy:
    // 1) Default OCCT STEP import (includes ShapeFix)
    // 2) If that fails/crashes, retry with ShapeFix disabled
    let bytes = match run_once(false) {
        Ok(bytes) => bytes,
        Err(e1) => match run_once(true) {
            Ok(bytes) => bytes,
            Err(e2) => {
                return Err(anyhow::anyhow!("step mesher failed (fixshape+nofixshape). first={e1} second={e2}"));
            }
        },
    };

    // Minimal sanity check: [u32 num_vertices][u32 num_indices]
    if bytes.len() < 8 {
        return Err(anyhow::anyhow!(
            "step mesher produced too little output: {} bytes",
            bytes.len()
        ));
    }

    Ok(bytes)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let file_path = std::env::args().nth(1).unwrap_or("../examples/tube.lua".into());
    let file_path = PathBuf::from(file_path);

    info!("Watching: {:?}", file_path);

    // Get git branch
    let git_branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    info!("Git branch: {}", git_branch);

    // Get port early so we can include it in AppState
    let port: u16 = std::env::var("MITTENS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3001);

    let (mesh_tx, _) = broadcast::channel::<Packet>(16);
    let (labels_tx, _) = broadcast::channel::<String>(16);
    let (exports_tx, _) = broadcast::channel::<String>(16);
    let (project_tx, project_rx) = mpsc::unbounded_channel::<PathBuf>();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<Packet>();
    let (labels_result_tx, mut labels_result_rx) = mpsc::unbounded_channel::<String>();
    let (exports_result_tx, mut exports_result_rx) = mpsc::unbounded_channel::<Vec<String>>();
    let (scene_result_tx, mut scene_result_rx) = mpsc::unbounded_channel::<Option<ir::SceneIr>>();

    // Lua processing thread (Manifold needs to run on same thread)
    let labels_tx_clone = labels_result_tx.clone();
    let exports_tx_clone = exports_result_tx.clone();
    let scene_tx_clone = scene_result_tx.clone();
    thread::spawn(move || {
        process_project_files(project_rx, result_tx, labels_tx_clone, exports_tx_clone, scene_tx_clone);
    });

    let state = Arc::new(AppState {
        mesh_tx: mesh_tx.clone(),
        labels_tx: labels_tx.clone(),
        exports_tx: exports_tx.clone(),
        current_view: RwLock::new(None),
        current_mesh: RwLock::new(None),
        current_field: RwLock::new(None),
        current_circuit: RwLock::new(None),
        current_nanovna: RwLock::new(None),
        current_labels: RwLock::new(None),
        current_exports: RwLock::new(Vec::new()),
        current_scene: RwLock::new(None),
        watched_file: file_path.display().to_string(),
        git_branch,
        port,
    });

    // Handle mesh/field/circuit/nanovna results
    let state_clone = state.clone();
    tokio::spawn(async move {
        while let Some(data) = result_rx.recv().await {
            let bytes = data.as_ref();
            let is_field = bytes.len() >= 5 && &bytes[0..5] == b"FIELD";
            let is_circuit = bytes.len() >= 8 && &bytes[0..8] == b"CIRCUIT\0";
            let is_nanovna = bytes.len() >= 8 && &bytes[0..8] == b"NANOVNA\0";
            let is_view = bytes.len() >= 4 && &bytes[0..4] == b"VIEW";

            if is_view {
                *state_clone.current_view.write().await = Some(data.clone());
            } else if is_field {
                *state_clone.current_field.write().await = Some(data.clone());
            } else if is_circuit {
                *state_clone.current_circuit.write().await = Some(data.clone());
            } else if is_nanovna {
                *state_clone.current_nanovna.write().await = Some(data.clone());
            } else {
                *state_clone.current_mesh.write().await = Some(data.clone());
            }
            let _ = state_clone.mesh_tx.send(data);
        }
    });

    // Handle labels results
    let state_clone = state.clone();
    tokio::spawn(async move {
        while let Some(labels_json) = labels_result_rx.recv().await {
            *state_clone.current_labels.write().await = Some(labels_json.clone());
            let _ = state_clone.labels_tx.send(labels_json);
        }
    });

    let state_clone = state.clone();
    tokio::spawn(async move {
        while let Some(exports) = exports_result_rx.recv().await {
            *state_clone.current_exports.write().await = exports.clone();
            let exports_json = serde_json::json!({
                "type": "exports",
                "files": exports
            });
            let _ = state_clone.exports_tx.send(exports_json.to_string());
        }
    });

    let state_clone = state.clone();
    tokio::spawn(async move {
        while let Some(scene) = scene_result_rx.recv().await {
            *state_clone.current_scene.write().await = scene.map(Arc::new);
        }
    });

    // Load initial file
    if file_path.exists() {
        let _ = project_tx.send(file_path.clone());
    }

    // File watcher
    let project_tx_clone = project_tx.clone();
    let watch_path = file_path.clone();
    tokio::spawn(async move {
        watch_file(watch_path, project_tx_clone).await;
    });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Server: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

struct CameraState {
    position: [f32; 3],
    target: [f32; 3],
    fov: f32,
    near: f32,
    far: f32,
}

fn serialize_view_config(flat_shading: bool, show_edges: bool, camera: Option<CameraState>) -> Vec<u8> {
    let mut data = Vec::with_capacity(49);
    data.extend_from_slice(b"VIEW\0\0\0\0");
    data.push(if flat_shading { 1 } else { 0 });
    data.push(if show_edges { 1 } else { 0 });

    match camera {
        Some(cam) => {
            data.push(1); // has_camera = 1
            data.extend_from_slice(&cam.position[0].to_le_bytes());
            data.extend_from_slice(&cam.position[1].to_le_bytes());
            data.extend_from_slice(&cam.position[2].to_le_bytes());
            data.extend_from_slice(&cam.target[0].to_le_bytes());
            data.extend_from_slice(&cam.target[1].to_le_bytes());
            data.extend_from_slice(&cam.target[2].to_le_bytes());
            data.extend_from_slice(&cam.fov.to_le_bytes());
            data.extend_from_slice(&cam.near.to_le_bytes());
            data.extend_from_slice(&cam.far.to_le_bytes());
        }
        None => {
            data.push(0); // has_camera = 0
        }
    }

    data
}

fn process_project_files(
    mut rx: mpsc::UnboundedReceiver<PathBuf>,
    tx: mpsc::UnboundedSender<Packet>,
    labels_tx: mpsc::UnboundedSender<String>,
    exports_tx: mpsc::UnboundedSender<Vec<String>>,
    scene_tx: mpsc::UnboundedSender<Option<ir::SceneIr>>,
) {
    let lua = mlua::Lua::new();

    // Set up package path to include stdlib directory
    let package_path = lua
        .globals()
        .get::<_, mlua::Table>("package")
        .and_then(|p| p.get::<_, String>("path"))
        .unwrap_or_default();

    let stdlib_path = "../?.lua;../?/init.lua";
    let new_path = format!("{};{}", stdlib_path, package_path);

    if let Ok(package) = lua.globals().get::<_, mlua::Table>("package") {
        let _ = package.set("path", new_path);
    }

    while let Some(file_path) = rx.blocking_recv() {
        let base_dir = file_path.parent().unwrap_or(std::path::Path::new("."));

        let kind = match cad_io::detect_project_kind(&file_path) {
            Ok(kind) => kind,
            Err(e) => {
                error!("Project file type error: {}", e);
                continue;
            }
        };

        let mut lua_content: Option<String> = None;
        let process_result = match kind {
            cad_io::ProjectKind::Lua => {
                let content = match std::fs::read_to_string(&file_path) {
                    Ok(content) => content,
                    Err(e) => {
                        error!("Failed to read Lua file {}: {}", file_path.display(), e);
                        continue;
                    }
                };
                lua_content = Some(content.clone());
                process_single_file(&lua, &content, base_dir)
            }
            cad_io::ProjectKind::Stl => {
                let file_name = file_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| file_path.display().to_string());
                cad_io::load_mesh_from_path(&file_path).map(|mesh| ProcessResult {
                    mesh,
                    flat_shading: false,
                    show_edges: false,
                    circular_segments: 32,
                    camera: None,
                    labels: Vec::new(),
                    exports: vec![file_name],
                    scene_ir: ir::SceneIr { kind: "scene".to_string(), objects: Vec::new() },
                })
            }
            cad_io::ProjectKind::Step => {
                let file_name = file_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| file_path.display().to_string());

                // Keep STEP meshing out-of-process; OCCT can OOM/abort on large assemblies.
                // View config first so the renderer can get into a known state immediately.
                // Default to edges off: edge overlay is expensive on very large assemblies.
                // We'll enable edges later for very small meshes (thin plates etc).
                let view_binary = serialize_view_config(false, false, None);
                let _ = tx.send(packet_from_vec(view_binary));

                let deflection_fine = std::env::var("MITTENS_STEP_DEFLECTION")
                    .ok()
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or_else(|| default_step_deflection_for_path(&file_path));
                let deflection_coarse = std::env::var("MITTENS_STEP_DEFLECTION_COARSE")
                    .ok()
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or_else(|| (deflection_fine * 3.0).max(deflection_fine));

                let refine = std::env::var("MITTENS_STEP_REFINE")
                    .ok()
                    .map(|v| {
                        let v = v.trim().to_ascii_lowercase();
                        v == "1" || v == "true" || v == "yes" || v == "on"
                    })
                    .unwrap_or(false);

                // First pass: coarse mesh for quick interactivity.
                // Emit a "cache hit" hint early so the user sees that we're not remeshing.
                let coarse_cache_hit_hint = step_mesh_cache_path(&file_path, deflection_coarse)
                    .ok()
                    .and_then(|p| std::fs::metadata(&p).ok().map(|m| (p, m.len())));
                let coarse_hint = if let Some((_p, len)) = coarse_cache_hit_hint {
                    format!(
                        "STEP coarse mesh cache hit (deflection={}) {:.1} MiB",
                        deflection_coarse,
                        (len as f64) / (1024.0 * 1024.0)
                    )
                } else {
                    format!("STEP meshing (coarse deflection={})", deflection_coarse)
                };
                let _ = labels_tx.send(
                    serde_json::json!({
                        "type": "status",
                        "state": "meshing",
                        "detail": coarse_hint,
                    })
                    .to_string(),
                );
                match mesh_step_cached_or_run(&file_path, deflection_coarse) {
                    Ok((mesh_binary, cache_hit)) => {
                        info!(
                            "STEP coarse mesh packet: {} bytes (deflection={}) cache_hit={}",
                            mesh_binary.len(),
                            deflection_coarse,
                            cache_hit
                        );
                        if let Some((nv, ni)) = parse_mesh_stats(&mesh_binary) {
                            let tris = (ni / 3) as u64;
                            let mb = (mesh_binary.len() as f64) / (1024.0 * 1024.0);
                            let _ = labels_tx.send(
                                serde_json::json!({
                                    "type": "status",
                                    "state": "meshing",
                                    "detail": format!("STEP coarse mesh {} ({} verts, {} tris, {:.1} MiB)", if cache_hit { "cache hit" } else { "built" }, nv, tris, mb),
                                })
                                .to_string(),
                            );

                            // Enable edges for small meshes; it helps thin plates show up, but is
                            // too expensive for large assemblies.
                            let show_edges = tris <= 20_000;
                            let view_binary = serialize_view_config(false, show_edges, None);
                            let _ = tx.send(packet_from_vec(view_binary));
                        }
                        let _ = tx.send(packet_from_vec(mesh_binary));
                        let _ = exports_tx.send(vec![file_name.clone()]);
                        let _ = scene_tx.send(None);
                        let _ = labels_tx.send(
                            serde_json::json!({
                                "type": "status",
                                "state": "ready",
                                "detail": "STEP coarse mesh ready",
                            })
                            .to_string(),
                        );
                    }
                    Err(e) => {
                        error!("STEP coarse meshing error: {}", e);
                        let _ = scene_tx.send(None);
                        let _ = labels_tx.send(
                            serde_json::json!({
                                "type": "status",
                                "state": "error",
                                "detail": format!("STEP meshing failed: {}", e),
                            })
                            .to_string(),
                        );
                    }
                };

                // Second pass: optional refinement, only if it is actually finer.
                if refine && deflection_fine < deflection_coarse {
                    let _ = labels_tx.send(
                        serde_json::json!({
                            "type": "status",
                            "state": "meshing",
                            "detail": format!("STEP meshing (refine deflection={})", deflection_fine),
                        })
                        .to_string(),
                    );
                    match mesh_step_cached_or_run(&file_path, deflection_fine) {
                        Ok((mesh_binary, cache_hit)) => {
                            info!(
                                "STEP refined mesh packet: {} bytes (deflection={}) cache_hit={}",
                                mesh_binary.len(),
                                deflection_fine,
                                cache_hit
                            );
                            if let Some((nv, ni)) = parse_mesh_stats(&mesh_binary) {
                                let tris = (ni / 3) as u64;
                                let mb = (mesh_binary.len() as f64) / (1024.0 * 1024.0);
                                let _ = labels_tx.send(
                                    serde_json::json!({
                                        "type": "status",
                                        "state": "meshing",
                                        "detail": format!("STEP refined mesh {} ({} verts, {} tris, {:.1} MiB)", if cache_hit { "cache hit" } else { "built" }, nv, tris, mb),
                                    })
                                    .to_string(),
                                );

                                let show_edges = tris <= 20_000;
                                let view_binary = serialize_view_config(false, show_edges, None);
                                let _ = tx.send(packet_from_vec(view_binary));
                            }
                            let _ = tx.send(packet_from_vec(mesh_binary));
                            let _ = exports_tx.send(vec![file_name]);
                            let _ = scene_tx.send(None);
                            let _ = labels_tx.send(
                                serde_json::json!({
                                    "type": "status",
                                    "state": "ready",
                                    "detail": "STEP refined mesh ready",
                                })
                                .to_string(),
                            );
                        }
                        Err(e) => {
                            error!("STEP refined meshing error: {}", e);
                            let _ = scene_tx.send(None);
                            let _ = labels_tx.send(
                                serde_json::json!({
                                    "type": "status",
                                    "state": "error",
                                    "detail": format!("STEP refine failed: {}", e),
                                })
                                .to_string(),
                            );
                        }
                    };
                }

                // STEP path doesn't use Lua post-processing (fields/circuits/etc).
                continue;
            }
        };

        match process_result {
            Ok(result) => {
                match kind {
                    cad_io::ProjectKind::Lua => {
                        let _ = scene_tx.send(Some(result.scene_ir.clone()));
                    }
                    _ => {
                        let _ = scene_tx.send(None);
                    }
                }
                // Send view config first
                let view_binary = serialize_view_config(result.flat_shading, result.show_edges, result.camera);
                let _ = tx.send(packet_from_vec(view_binary));

                let binary = result.mesh.to_binary();
                info!(
                    "Generated mesh: {} vertices, {} triangles, {} bytes, flat_shading={}",
                    result.mesh.positions.len() / 3,
                    result.mesh.indices.len() / 3,
                    binary.len(),
                    result.flat_shading
                );
                let _ = tx.send(packet_from_vec(binary));
                let _ = exports_tx.send(result.exports.clone());

                // Send labels as JSON if any
                if !result.labels.is_empty() {
                    let labels_json = serde_json::json!({
                        "type": "labels",
                        "labels": result.labels
                    });
                    if let Ok(json_str) = serde_json::to_string(&labels_json) {
                        let _ = labels_tx.send(json_str);
                    }
                }
            }
            Err(e) => {
                let _ = scene_tx.send(None);
                error!("Project processing error: {}", e);
            }
        }

        let Some(content) = lua_content.as_deref() else {
            continue;
        };

        // Try to compute magnetic field if this looks like a Helmholtz coil
        if let Some(field_data) = try_compute_helmholtz_field(&lua, content) {
            let field_binary = field_data.to_binary();
            info!(
                "Generated field: {}x{} slice, {} arrows, {} line points, {} bytes",
                field_data.slice_width,
                field_data.slice_height,
                field_data.arrows_positions.len() / 3,
                field_data.line_z.len(),
                field_binary.len()
            );
            let _ = tx.send(packet_from_vec(field_binary));
        }

        // Try to compute acoustic field if this looks like an acoustic simulation
        if let Some(field_data) = try_compute_acoustic_field(&lua, content) {
            let field_binary = field_data.to_binary();
            info!(
                "Generated acoustic field: {}x{} slice, {} bytes",
                field_data.slice_width,
                field_data.slice_height,
                field_binary.len()
            );
            let _ = tx.send(packet_from_vec(field_binary));
        }

        // Try to generate circuit diagram if this looks like a circuit definition
        if let Some(circuit_data) = try_generate_circuit(&lua, content) {
            let circuit_binary = circuit_data.to_binary();
            info!(
                "Generated circuit: {}x{}, {} bytes SVG",
                circuit_data.width,
                circuit_data.height,
                circuit_data.svg.len()
            );
            let _ = tx.send(packet_from_vec(circuit_binary));
        }

        // Compute GaussMeter point measurements
        let gaussmeter_measurements = try_compute_gaussmeter_measurements(&lua, content);
        for m in &gaussmeter_measurements {
            let binary = m.to_binary();
            let _ = tx.send(packet_from_vec(binary));
        }

        // Compute Probe line measurements
        let probe_measurements = try_compute_probe_measurements(&lua, content);
        for m in &probe_measurements {
            let binary = m.to_binary();
            let _ = tx.send(packet_from_vec(binary));
        }

        // Compute Hydrophone point measurements and send to renderer
        let hydrophone_measurements = try_compute_hydrophone_measurements(&lua, content);
        for (x, y, z, magnitude, label) in &hydrophone_measurements {
            info!(
                "Hydrophone measurement '{}': position=({:.1}, {:.1}, {:.1}), magnitude={:.6}",
                label, x, y, z, magnitude
            );
            // Convert to PointMeasurement and send to renderer
            let measurement = field::PointMeasurement {
                position: [*x, *y, *z],
                value: [*magnitude, 0.0, 0.0], // Acoustic pressure is scalar, stored in first component
                magnitude: *magnitude,
                label: label.clone(),
            };
            let binary = measurement.to_binary();
            let _ = tx.send(packet_from_vec(binary));
        }

        // Compute NanoVNA frequency sweep if configured
        if let Some(sweep) = try_compute_nanovna_sweep(&lua, content) {
            let sweep_binary = sweep.to_binary();
            info!(
                "Generated NanoVNA sweep: {} points, {} bytes",
                sweep.points.len(),
                sweep_binary.len()
            );
            let _ = tx.send(packet_from_vec(sweep_binary));
        }
    }
}

fn parse_plane_type(plane_str: &str) -> field::PlaneType {
    match plane_str.to_uppercase().as_str() {
        "XY" => field::PlaneType::XY,
        "YZ" => field::PlaneType::YZ,
        _ => field::PlaneType::XZ,
    }
}

fn get_field_plane_config(lua: &mlua::Lua, instrument_type: &str) -> (field::PlaneType, f64, field::Colormap) {
    let globals = lua.globals();

    let instruments: mlua::Table = match globals.get("Instruments") {
        Ok(t) => t,
        Err(_) => return (field::PlaneType::XZ, 0.0, field::Colormap::Jet),
    };

    let active: mlua::Table = match instruments.get("_active") {
        Ok(t) => t,
        Err(_) => return (field::PlaneType::XZ, 0.0, field::Colormap::Jet),
    };

    for pair in active.pairs::<i64, mlua::Table>() {
        let (_, inst) = match pair {
            Ok(p) => p,
            Err(_) => continue,
        };

        let inst_type: String = match inst.get("_instrument_type") {
            Ok(t) => t,
            Err(_) => continue,
        };

        if inst_type == instrument_type {
            let config: mlua::Table = match inst.get("_config") {
                Ok(c) => c,
                Err(_) => continue,
            };

            let plane_str: String = config.get("plane").unwrap_or_else(|_| "XZ".to_string());
            let offset: f64 = config.get("offset").unwrap_or(0.0);
            let colormap_str: String = config.get("color_map").unwrap_or_else(|_| "jet".to_string());

            return (parse_plane_type(&plane_str), offset, field::Colormap::from_str(&colormap_str));
        }
    }

    (field::PlaneType::XZ, 0.0, field::Colormap::Jet)
}

fn try_compute_helmholtz_field(lua: &mlua::Lua, content: &str) -> Option<field::FieldData> {
    if !content.contains("helmholtz") && !content.contains("Coil") && !content.contains("coil_mean_radius") {
        return None;
    }

    let result: mlua::Value = lua.load(content).eval().ok()?;
    let _table = result.as_table()?;

    let globals = lua.globals();

    // Try "Coil" global first (project convention), then fall back to "config"
    let (coil_mean_radius, gap, windings, layers, current) = if let Ok(coil) = globals.get::<_, mlua::Table>("Coil") {
        let mean_radius: f64 = coil.get("mean_radius").ok()?;
        let gap: f64 = coil.get("gap").ok()?;
        let windings: f64 = coil.get("windings").unwrap_or(100.0);
        let layers: f64 = coil.get("layers").unwrap_or(10.0);
        let current: f64 = coil.get("current").unwrap_or(1.0);
        (mean_radius, gap, windings, layers, current)
    } else if let Ok(config) = globals.get::<_, mlua::Table>("config") {
        let mean_radius: f64 = config.get("coil_mean_radius").ok()?;
        let gap: f64 = config.get("gap").ok()?;
        let windings: f64 = config.get("windings").unwrap_or(100.0);
        let layers: f64 = config.get("layers").unwrap_or(10.0);
        let current: f64 = config.get("current").unwrap_or(1.0);
        (mean_radius, gap, windings, layers, current)
    } else {
        return None;
    };

    // Try to get Wire config for packing info
    let (wire_diameter, packing_factor) = if let Ok(wire) = globals.get::<_, mlua::Table>("Wire") {
        let diameter: f64 = wire.get("diameter").unwrap_or(0.8);
        let packing: f64 = wire.get("packing_factor").unwrap_or(0.82);
        (diameter, packing)
    } else {
        (0.8, 0.82)
    };

    let turns_per_layer = (windings / layers).ceil();
    let wire_pitch = wire_diameter / packing_factor;
    let coil_width = turns_per_layer * wire_pitch;
    let coil_height = layers * wire_pitch;
    let coil_inner_r = coil_mean_radius - coil_height / 2.0;
    let coil_outer_r = coil_mean_radius + coil_height / 2.0;
    let ampere_turns = current * windings;

    let (plane_type, plane_offset, colormap) = get_field_plane_config(lua, "field_plane");

    info!(
        "Computing Helmholtz field: R={:.1}mm, gap={:.1}mm, {:.0} A·turns, plane={:?}, offset={:.1}mm, colormap={:?}",
        coil_mean_radius, gap, ampere_turns, plane_type, plane_offset, colormap
    );

    Some(field::compute_helmholtz_field(
        coil_mean_radius,
        coil_inner_r,
        coil_outer_r,
        coil_width,
        gap,
        ampere_turns,
        layers as usize,
        plane_type,
        plane_offset,
        colormap,
    ))
}

fn try_compute_acoustic_field(lua: &mlua::Lua, content: &str) -> Option<field::FieldData> {
    // Check if this file defines acoustic simulation configuration
    let has_acoustic = content.contains("acoustic(")
        || content.contains("Acoustic")
        || content.contains("Transducer")
        || content.contains("Medium");

    if !has_acoustic {
        return None;
    }

    // Execute the Lua to get config values
    let _result: mlua::Value = lua.load(content).eval().ok()?;
    let globals = lua.globals();

    // Try to get Acoustic config
    let acoustic: mlua::Table = globals.get("Acoustic").ok()?;
    let frequency: f64 = acoustic.get("frequency").unwrap_or(1e6);
    let drive_amplitude: f64 = acoustic.get("drive_current").unwrap_or(1.0);

    // Try to get Transducer config
    let transducer: mlua::Table = globals.get("Transducer").ok()?;
    let transducer_diameter: f64 = transducer.get("diameter").unwrap_or(12.0);
    let transducer_z: f64 = transducer.get("height_from_coverslip").unwrap_or(5.0);

    // Try to get PolyTube config for medium radius
    let medium_radius: f64 = if let Ok(polytube) = globals.get::<_, mlua::Table>("PolyTube") {
        polytube.get::<_, f64>("inner_diameter").unwrap_or(26.0) / 2.0
    } else {
        13.0
    };

    // Try to get Medium config for liquid height
    let medium_height: f64 = if let Ok(medium) = globals.get::<_, mlua::Table>("Medium") {
        medium.get::<_, f64>("liquid_height").unwrap_or(8.0)
    } else {
        8.0
    };

    let config = acoustic::AcousticConfig {
        frequency,
        transducer_radius: transducer_diameter / 2.0,
        transducer_z,
        medium_radius,
        medium_height,
        speed_of_sound: 1480.0 * 1000.0,
        drive_amplitude,
    };

    let (plane_type, plane_offset, colormap) = get_field_plane_config(lua, "acoustic_pressure_plane");

    info!(
        "Computing acoustic field: f={:.0}Hz, R={:.1}mm, z={:.1}mm, plane={:?}, offset={:.1}mm, colormap={:?}",
        config.frequency, config.transducer_radius, config.transducer_z, plane_type, plane_offset, colormap
    );

    Some(acoustic::compute_acoustic_field(&config, plane_type, plane_offset, colormap))
}

fn try_compute_probe_measurements(lua: &mlua::Lua, content: &str) -> Vec<field::LineMeasurement> {
    let mut measurements = Vec::new();

    if !content.contains("helmholtz") && !content.contains("Coil") && !content.contains("coil_mean_radius") {
        return measurements;
    }

    let globals = lua.globals();

    let (coil_mean_radius, gap, windings, layers, current) = if let Ok(coil) = globals.get::<_, mlua::Table>("Coil") {
        let mean_radius: f64 = match coil.get("mean_radius") {
            Ok(v) => v,
            Err(_) => return measurements,
        };
        let gap: f64 = coil.get("gap").unwrap_or(mean_radius);
        let windings: f64 = coil.get("windings").unwrap_or(100.0);
        let layers: f64 = coil.get("layers").unwrap_or(10.0);
        let current: f64 = coil.get("current").unwrap_or(1.0);
        (mean_radius, gap, windings, layers, current)
    } else if let Ok(config) = globals.get::<_, mlua::Table>("config") {
        let mean_radius: f64 = match config.get("coil_mean_radius") {
            Ok(v) => v,
            Err(_) => return measurements,
        };
        let gap: f64 = config.get("gap").unwrap_or(mean_radius);
        let windings: f64 = config.get("windings").unwrap_or(100.0);
        let layers: f64 = config.get("layers").unwrap_or(10.0);
        let current: f64 = config.get("current").unwrap_or(1.0);
        (mean_radius, gap, windings, layers, current)
    } else {
        return measurements;
    };

    let (wire_diameter, packing_factor) = if let Ok(wire) = globals.get::<_, mlua::Table>("Wire") {
        let diameter: f64 = wire.get("diameter").unwrap_or(0.8);
        let packing: f64 = wire.get("packing_factor").unwrap_or(0.82);
        (diameter, packing)
    } else {
        (0.8, 0.82)
    };

    let turns_per_layer = (windings / layers).ceil();
    let wire_pitch = wire_diameter / packing_factor;
    let coil_height = layers * wire_pitch;
    let coil_inner_r = coil_mean_radius - coil_height / 2.0;
    let coil_outer_r = coil_mean_radius + coil_height / 2.0;
    let coil_width = turns_per_layer * wire_pitch;
    let ampere_turns = current * windings;

    let instruments: mlua::Table = match globals.get("Instruments") {
        Ok(t) => t,
        Err(_) => return measurements,
    };

    let active: mlua::Table = match instruments.get("_active") {
        Ok(t) => t,
        Err(_) => return measurements,
    };

    for pair in active.pairs::<i64, mlua::Table>() {
        let (_, inst) = match pair {
            Ok(p) => p,
            Err(_) => continue,
        };

        let inst_type: String = match inst.get("_instrument_type") {
            Ok(t) => t,
            Err(_) => continue,
        };

        if inst_type != "probe" {
            continue;
        }

        let config_table: mlua::Table = match inst.get("_config") {
            Ok(c) => c,
            Err(_) => continue,
        };

        let probe_type: String = config_table.get("type").unwrap_or_else(|_| "B_field".to_string());
        if probe_type != "B_field" {
            continue;
        }

        let line_table: mlua::Table = match config_table.get("line") {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Lua API uses array format: line = { {x1,y1,z1}, {x2,y2,z2} }
        let start_table: mlua::Table = match line_table.get(1) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let stop_table: mlua::Table = match line_table.get(2) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let start: [f64; 3] = [
            start_table.get(1).unwrap_or(0.0),
            start_table.get(2).unwrap_or(0.0),
            start_table.get(3).unwrap_or(0.0),
        ];

        let stop: [f64; 3] = [
            stop_table.get(1).unwrap_or(0.0),
            stop_table.get(2).unwrap_or(0.0),
            stop_table.get(3).unwrap_or(0.0),
        ];

        let num_points: usize = config_table.get::<_, u32>("points").unwrap_or(51) as usize;
        let name: String = config_table.get("name").unwrap_or_else(|_| "probe".to_string());

        let mut positions = Vec::with_capacity(num_points * 3);
        let mut values = Vec::with_capacity(num_points * 3);
        let mut magnitudes = Vec::with_capacity(num_points);

        for i in 0..num_points {
            let t = if num_points > 1 { i as f64 / (num_points - 1) as f64 } else { 0.5 };
            let point = [
                start[0] + t * (stop[0] - start[0]),
                start[1] + t * (stop[1] - start[1]),
                start[2] + t * (stop[2] - start[2]),
            ];

            let b = field::compute_point_field(
                coil_inner_r,
                coil_outer_r,
                coil_width,
                gap,
                ampere_turns,
                layers as usize,
                point,
            );

            let magnitude = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();

            positions.push(point[0] as f32);
            positions.push(point[1] as f32);
            positions.push(point[2] as f32);
            values.push(b[0] as f32);
            values.push(b[1] as f32);
            values.push(b[2] as f32);
            magnitudes.push(magnitude as f32);
        }

        let statistics = if config_table.get::<_, mlua::Table>("statistics").is_ok() {
            let n = magnitudes.len() as f32;
            let sum: f32 = magnitudes.iter().sum();
            let mean = sum / n;
            let variance: f32 = magnitudes.iter().map(|&x| (x - mean) * (x - mean)).sum::<f32>() / n;
            let std = variance.sqrt();
            let min = magnitudes.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = magnitudes.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            info!(
                "Probe '{}' statistics: min={:.4}, max={:.4}, mean={:.4}, std={:.4}",
                name, min, max, mean, std
            );
            Some(field::ProbeStatistics { min, max, mean, std })
        } else {
            None
        };

        info!(
            "Probe '{}': {} points from ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1})",
            name, num_points, start[0], start[1], start[2], stop[0], stop[1], stop[2]
        );

        measurements.push(field::LineMeasurement {
            name,
            start,
            stop,
            positions,
            values,
            magnitudes,
            statistics,
        });
    }

    measurements
}

fn try_compute_gaussmeter_measurements(lua: &mlua::Lua, content: &str) -> Vec<field::PointMeasurement> {
    let mut measurements = Vec::new();

    if !content.contains("helmholtz") && !content.contains("Coil") && !content.contains("coil_mean_radius") {
        return measurements;
    }

    let globals = lua.globals();
    let (coil_mean_radius, gap, windings, layers, current) = if let Ok(coil) = globals.get::<_, mlua::Table>("Coil") {
        let mean_radius: f64 = match coil.get("mean_radius") {
            Ok(v) => v,
            Err(_) => return measurements,
        };
        let gap: f64 = coil.get("gap").unwrap_or(mean_radius);
        let windings: f64 = coil.get("windings").unwrap_or(100.0);
        let layers: f64 = coil.get("layers").unwrap_or(10.0);
        let current: f64 = coil.get("current").unwrap_or(1.0);
        (mean_radius, gap, windings, layers, current)
    } else if let Ok(config) = globals.get::<_, mlua::Table>("config") {
        let mean_radius: f64 = match config.get("coil_mean_radius") {
            Ok(v) => v,
            Err(_) => return measurements,
        };
        let gap: f64 = config.get("gap").unwrap_or(mean_radius);
        let windings: f64 = config.get("windings").unwrap_or(100.0);
        let layers: f64 = config.get("layers").unwrap_or(10.0);
        let current: f64 = config.get("current").unwrap_or(1.0);
        (mean_radius, gap, windings, layers, current)
    } else {
        return measurements;
    };

    // Get Wire config for packing info
    let (wire_diameter, packing_factor) = if let Ok(wire) = globals.get::<_, mlua::Table>("Wire") {
        let diameter: f64 = wire.get("diameter").unwrap_or(0.8);
        let packing: f64 = wire.get("packing_factor").unwrap_or(0.82);
        (diameter, packing)
    } else {
        (0.8, 0.82)
    };

    let turns_per_layer = (windings / layers).ceil();
    let wire_pitch = wire_diameter / packing_factor;
    let coil_height = layers * wire_pitch;
    let coil_inner_r = coil_mean_radius - coil_height / 2.0;
    let coil_outer_r = coil_mean_radius + coil_height / 2.0;
    let coil_width = turns_per_layer * wire_pitch;
    let ampere_turns = current * windings;

    // Find GaussMeter instruments
    let instruments: mlua::Table = match globals.get("Instruments") {
        Ok(t) => t,
        Err(_) => return measurements,
    };

    let active: mlua::Table = match instruments.get("_active") {
        Ok(t) => t,
        Err(_) => return measurements,
    };

    for pair in active.pairs::<i64, mlua::Table>() {
        let (_, inst) = match pair {
            Ok(p) => p,
            Err(_) => continue,
        };

        let inst_type: String = match inst.get("_instrument_type") {
            Ok(t) => t,
            Err(_) => continue,
        };

        if inst_type != "gaussmeter" {
            continue;
        }

        let position: mlua::Table = match inst.get("_position") {
            Ok(p) => p,
            Err(_) => continue,
        };

        let x: f64 = position.get(1).unwrap_or(0.0);
        let y: f64 = position.get(2).unwrap_or(0.0);
        let z: f64 = position.get(3).unwrap_or(0.0);

        let config_table: mlua::Table = inst.get("_config").unwrap_or_else(|_| lua.create_table().unwrap());
        let label: String = config_table.get("label").unwrap_or_else(|_| "B".to_string());

        let b = field::compute_point_field(
            coil_inner_r,
            coil_outer_r,
            coil_width,
            gap,
            ampere_turns,
            layers as usize,
            [x, y, z],
        );

        let magnitude = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();

        info!(
            "GaussMeter '{}' at ({:.1}, {:.1}, {:.1}): B = {:.4} mT",
            label, x, y, z, magnitude * 1000.0
        );

        measurements.push(field::PointMeasurement {
            position: [x, y, z],
            value: b,
            magnitude,
            label,
        });
    }

    measurements
}

/// Process Hydrophone instruments and compute pressure at their positions
fn try_compute_hydrophone_measurements(lua: &mlua::Lua, content: &str) -> Vec<(f64, f64, f64, f64, String)> {
    let mut measurements = Vec::new();

    let has_acoustic = content.contains("acoustic(")
        || content.contains("Acoustic")
        || content.contains("Transducer")
        || content.contains("Medium");

    if !has_acoustic {
        return measurements;
    }

    let globals = lua.globals();

    // Get acoustic configuration
    let acoustic_table: mlua::Table = match globals.get("Acoustic") {
        Ok(t) => t,
        Err(_) => return measurements,
    };
    let frequency: f64 = acoustic_table.get("frequency").unwrap_or(1e6);
    let drive_amplitude: f64 = acoustic_table.get("drive_current").unwrap_or(1.0);

    let transducer: mlua::Table = match globals.get("Transducer") {
        Ok(t) => t,
        Err(_) => return measurements,
    };
    let transducer_diameter: f64 = transducer.get("diameter").unwrap_or(12.0);
    let transducer_z: f64 = transducer.get("height_from_coverslip").unwrap_or(5.0);

    let medium_radius: f64 = if let Ok(polytube) = globals.get::<_, mlua::Table>("PolyTube") {
        polytube.get::<_, f64>("inner_diameter").unwrap_or(26.0) / 2.0
    } else {
        13.0
    };

    let medium_height: f64 = if let Ok(medium) = globals.get::<_, mlua::Table>("Medium") {
        medium.get::<_, f64>("liquid_height").unwrap_or(8.0)
    } else {
        8.0
    };

    let config = acoustic::AcousticConfig {
        frequency,
        transducer_radius: transducer_diameter / 2.0,
        transducer_z,
        medium_radius,
        medium_height,
        speed_of_sound: 1480.0 * 1000.0,
        drive_amplitude,
    };

    // Find Hydrophone instruments
    let instruments: mlua::Table = match globals.get("Instruments") {
        Ok(t) => t,
        Err(_) => return measurements,
    };

    let active: mlua::Table = match instruments.get("_active") {
        Ok(t) => t,
        Err(_) => return measurements,
    };

    for pair in active.pairs::<i64, mlua::Table>() {
        let (_, inst) = match pair {
            Ok(p) => p,
            Err(_) => continue,
        };

        let inst_type: String = match inst.get("_instrument_type") {
            Ok(t) => t,
            Err(_) => continue,
        };

        if inst_type != "hydrophone" {
            continue;
        }

        let position: mlua::Table = match inst.get("_position") {
            Ok(p) => p,
            Err(_) => continue,
        };

        let x: f64 = position.get(1).unwrap_or(0.0);
        let y: f64 = position.get(2).unwrap_or(0.0);
        let z: f64 = position.get(3).unwrap_or(0.0);

        let config_table: mlua::Table = inst.get("_config").unwrap_or_else(|_| lua.create_table().unwrap());
        let label: String = config_table.get("label").unwrap_or_else(|_| "P".to_string());

        // Convert position to cylindrical (r, z) for acoustic computation
        let r = (x * x + y * y).sqrt();
        let (p_real, p_imag) = acoustic::compute_pressure_at_point(r, z, &config);
        let magnitude = (p_real * p_real + p_imag * p_imag).sqrt();

        info!(
            "Hydrophone '{}' at ({:.1}, {:.1}, {:.1}): P = {:.4} (normalized)",
            label, x, y, z, magnitude
        );

        measurements.push((x, y, z, magnitude, label));
    }

    measurements
}

/// Process NanoVNA frequency sweep if configured
fn try_compute_nanovna_sweep(lua: &mlua::Lua, content: &str) -> Option<nanovna::FrequencySweep> {
    if !content.contains("NanoVNA") && !content.contains("nanovna") {
        return None;
    }

    let _result: mlua::Value = lua.load(content).eval().ok()?;
    let globals = lua.globals();

    let nanovna_table: mlua::Table = globals.get("NanoVNA").ok()?;

    let f_start: f64 = nanovna_table.get("f_start").unwrap_or(1e6);
    let f_stop: f64 = nanovna_table.get("f_stop").unwrap_or(50e6);
    let num_points: usize = nanovna_table.get::<_, u32>("num_points").unwrap_or(101) as usize;

    // Get coil configuration
    let coil_radius: f64 = nanovna_table.get("coil_radius").unwrap_or(25.0);
    let num_turns: u32 = nanovna_table.get("num_turns").unwrap_or(10);
    let wire_diameter: f64 = nanovna_table.get("wire_diameter").unwrap_or(0.5);
    let coil_resistance: f64 = nanovna_table.get("coil_resistance").unwrap_or(0.5);

    let config = nanovna::NanoVNAConfig {
        f_start,
        f_stop,
        num_points,
        coil_radius,
        num_turns,
        wire_diameter,
        coil_resistance,
        parasitic_capacitance_pf: None,
        resonator_radius: None,
        resonator_distance: 10.0,
        resonator_resistance: 0.1,
    };

    info!(
        "Computing NanoVNA sweep: {:.2} MHz - {:.2} MHz, {} points, R={:.1}mm, N={}",
        f_start / 1e6, f_stop / 1e6, num_points, coil_radius, num_turns
    );

    let sweep = nanovna::compute_frequency_sweep(&config);

    info!(
        "NanoVNA min S11: {:.2} dB at {:.3} MHz",
        sweep.min_s11_db, sweep.min_s11_freq / 1e6
    );

    Some(sweep)
}

fn try_generate_circuit(lua: &mlua::Lua, content: &str) -> Option<circuit::CircuitData> {
    if !content.contains("Circuit") {
        return None;
    }

    let _: mlua::Value = lua.load(content).eval().ok()?;
    let globals = lua.globals();

    let circuit_table: mlua::Table = globals.get("_circuit_data").ok()?;
    let components_table: mlua::Table = circuit_table.get("components").ok()?;
    let size_table: mlua::Table = circuit_table.get("size").ok()?;

    let width: f64 = size_table.get(1).unwrap_or(400.0);
    let height: f64 = size_table.get(2).unwrap_or(90.0);

    let mut components = Vec::new();

    for comp_result in components_table.sequence_values::<mlua::Table>() {
        let comp_table = comp_result.ok()?;
        let comp_type: String = comp_table.get("component").ok()?;
        let config: mlua::Table = comp_table.get("config").ok()?;

        let component = match comp_type.as_str() {
            "signal_generator" => {
                let frequency: f64 = config.get("frequency").unwrap_or(1e6);
                let amplitude: f64 = config.get("amplitude").unwrap_or(1.0);
                circuit::CircuitComponent::SignalGenerator { frequency, amplitude }
            }
            "amplifier" => {
                let gain: f64 = config.get("gain").unwrap_or(10.0);
                circuit::CircuitComponent::Amplifier { gain }
            }
            "matching_network" => {
                let frequency: f64 = config.get("frequency").unwrap_or(1e6);
                let use_nanovna: bool = config.get("use_nanovna").unwrap_or(false);

                let (impedance_real, impedance_imag) = if use_nanovna {
                    if let Ok(nanovna_table) = globals.get::<_, mlua::Table>("NanoVNA") {
                        let nanovna_config = nanovna::NanoVNAConfig {
                            f_start: nanovna_table.get("f_start").unwrap_or(1e6),
                            f_stop: nanovna_table.get("f_stop").unwrap_or(50e6),
                            num_points: nanovna_table.get::<_, u32>("num_points").unwrap_or(101) as usize,
                            coil_radius: nanovna_table.get("coil_radius").unwrap_or(25.0),
                            num_turns: nanovna_table.get("num_turns").unwrap_or(10),
                            wire_diameter: nanovna_table.get("wire_diameter").unwrap_or(0.5),
                            coil_resistance: nanovna_table.get("coil_resistance").unwrap_or(0.5),
                            parasitic_capacitance_pf: None,
                            resonator_radius: None,
                            resonator_distance: 10.0,
                            resonator_resistance: 0.1,
                        };
                        let (z_real, z_imag) = nanovna::compute_impedance_at_frequency(&nanovna_config, frequency);
                        info!("MatchingNetwork using NanoVNA impedance at {:.2} MHz: Z = {:.2} + j{:.2} Ohm", frequency / 1e6, z_real, z_imag);
                        (z_real, z_imag)
                    } else {
                        info!("MatchingNetwork use_nanovna=true but no NanoVNA config found, using defaults");
                        (config.get("impedance_real").unwrap_or(50.0), config.get("impedance_imag").unwrap_or(0.0))
                    }
                } else {
                    (config.get("impedance_real").unwrap_or(50.0), config.get("impedance_imag").unwrap_or(0.0))
                };

                circuit::CircuitComponent::MatchingNetwork { impedance_real, impedance_imag, frequency }
            }
            "transducer_load" => {
                let impedance_real: f64 = config.get("impedance_real").unwrap_or(50.0);
                let impedance_imag: f64 = config.get("impedance_imag").unwrap_or(0.0);
                circuit::CircuitComponent::TransducerLoad { impedance_real, impedance_imag }
            }
            _ => continue,
        };

        components.push(component);
    }

    if components.is_empty() {
        return None;
    }

    info!(
        "Generating circuit diagram: {} components, {}x{}",
        components.len(), width, height
    );

    Some(circuit::generate_circuit_svg(&components, width, height))
}

struct ProcessResult {
    mesh: geometry::MeshData,
    flat_shading: bool,
    show_edges: bool,
    #[allow(dead_code)]
    circular_segments: u32,
    camera: Option<CameraState>,
    labels: Vec<Label>,
    exports: Vec<String>,
    scene_ir: ir::SceneIr,
}

#[derive(Clone, serde::Serialize)]
struct Label {
    text: String,
    x: f32,
    y: f32,
    z: f32,
    size: f32,
    color: String,
    ops: Vec<LabelOp>,
}

#[derive(Clone, serde::Serialize)]
struct LabelOp {
    op: String,
    x: f32,
    y: f32,
    z: f32,
}

fn process_single_file(lua: &mlua::Lua, content: &str, base_dir: &std::path::Path) -> Result<ProcessResult> {
    // Clear scene state before each execution to prevent accumulation
    let _ = lua.load(r#"
        local loaded = package.loaded["stdlib"] or package.loaded["stdlib.init"]
        if loaded and loaded.clear then loaded.clear() end
    "#).exec();

    // Expose the project directory so Lua can resolve relative asset paths (e.g. imported STLs).
    // This is intentionally a plain string; stdlib handles joining.
    lua.globals()
        .set("MITTENS_PROJECT_DIR", base_dir.to_string_lossy().to_string())?;

    let result: mlua::Value = lua.load(content).eval()?;

    // Extract view config
    let (flat_shading, show_edges, circular_segments, camera) = if let Some(table) = result.as_table() {
        if let Ok(view) = table.get::<_, mlua::Table>("view") {
            let flat = view.get::<_, bool>("flat_shading").unwrap_or(false);
            let edges = view.get::<_, bool>("show_edges").unwrap_or(false);
            let segments = view.get::<_, u32>("circular_segments").unwrap_or(32);

            let cam = if let Ok(cam_table) = view.get::<_, mlua::Table>("camera") {
                let pos: Option<mlua::Table> = cam_table.get("position").ok();
                let tgt: Option<mlua::Table> = cam_table.get("target").ok();
                let fov: Option<f32> = cam_table.get("fov").ok();
                let near: f32 = cam_table.get("near").unwrap_or(0.1);
                let far: f32 = cam_table.get("far").unwrap_or(100000.0);

                if let (Some(pos_t), Some(tgt_t), Some(fov_v)) = (pos, tgt, fov) {
                    let position = [
                        pos_t.get::<_, f32>(1).unwrap_or(100.0),
                        pos_t.get::<_, f32>(2).unwrap_or(100.0),
                        pos_t.get::<_, f32>(3).unwrap_or(100.0),
                    ];
                    let target = [
                        tgt_t.get::<_, f32>(1).unwrap_or(0.0),
                        tgt_t.get::<_, f32>(2).unwrap_or(0.0),
                        tgt_t.get::<_, f32>(3).unwrap_or(0.0),
                    ];
                    Some(CameraState { position, target, fov: fov_v, near, far })
                } else {
                    None
                }
            } else {
                None
            };

            (flat, edges, segments, cam)
        } else {
            (false, false, 32, None)
        }
    } else {
        (false, false, 32, None)
    };

    let scene_ir = ir::scene_from_lua_value(&result)?;
    let scene_hash = ir::scene_hash(&scene_ir).unwrap_or_else(|_| "unknown".to_string());
    info!(
        "Using IR -> Manifold backend, circular_segments={}, scene_hash={}",
        circular_segments, scene_hash
    );
    let mesh = geometry::generate_mesh_from_ir_scene(lua, &scene_ir, circular_segments)?;

    let exports = if let Some(table) = result.as_table() {
        export::process_exports_from_table(lua, table, &scene_ir, base_dir)
    } else {
        Vec::new()
    };

    if !exports.is_empty() {
        info!("Detected {} STL export(s): {:?}", exports.len(), exports);
    }

    // Extract labels
    let labels = if let Some(table) = result.as_table() {
        if let Ok(labels_table) = table.get::<_, mlua::Table>("labels") {
            let mut labels = Vec::new();
            for pair in labels_table.pairs::<i32, mlua::Table>() {
                if let Ok((_, label)) = pair {
                    let text: String = label.get("text").unwrap_or_default();
                    let x: f32 = label.get("x").unwrap_or(0.0);
                    let y: f32 = label.get("y").unwrap_or(0.0);
                    let z: f32 = label.get("z").unwrap_or(0.0);
                    let size: f32 = label.get("size").unwrap_or(5.0);
                    let color: String = label.get("color").unwrap_or_else(|_| "#ffffff".to_string());
                    let mut ops: Vec<LabelOp> = Vec::new();
                    if let Ok(ops_table) = label.get::<_, mlua::Table>("ops") {
                        for op_item in ops_table.sequence_values::<mlua::Table>() {
                            if let Ok(op_table) = op_item {
                                let op: String = op_table.get("op").unwrap_or_default();
                                if op.is_empty() {
                                    continue;
                                }
                                let x: f32 = op_table.get("x").unwrap_or(0.0);
                                let y: f32 = op_table.get("y").unwrap_or(0.0);
                                let z: f32 = op_table.get("z").unwrap_or(0.0);
                                ops.push(LabelOp { op, x, y, z });
                            }
                        }
                    }
                    labels.push(Label { text, x, y, z, size, color, ops });
                }
            }
            info!("Extracted {} labels", labels.len());
            labels
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    Ok(ProcessResult { mesh, flat_shading, show_edges, circular_segments, camera, labels, exports, scene_ir })
}

async fn watch_file(path: PathBuf, tx: mpsc::UnboundedSender<PathBuf>) {
    let (notify_tx, mut notify_rx) = mpsc::channel::<PathBuf>(10);

    let mut debouncer = new_debouncer(Duration::from_millis(200), move |res: DebounceEventResult| {
        if let Ok(events) = res {
            for event in events {
                let _ = notify_tx.blocking_send(event.path);
            }
        }
    })
    .unwrap();

    let watch_dir = path.parent().unwrap_or(&path);
    debouncer.watcher().watch(watch_dir, RecursiveMode::NonRecursive).unwrap();

    info!("Watching directory: {:?}", watch_dir);

    while let Some(changed) = notify_rx.recv().await {
        if changed == path || changed.file_name() == path.file_name() {
            info!("File changed, regenerating mesh...");
            let _ = tx.send(path.clone());
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

fn collect_export_stl_files(watched_file: &str, exports: &[String]) -> Vec<PathBuf> {
    let base_dir = PathBuf::from(watched_file)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let base_canon = std::fs::canonicalize(&base_dir).unwrap_or(base_dir.clone());

    let mut files = Vec::new();
    for rel in exports {
        let lower = rel.to_ascii_lowercase();
        if !(lower.ends_with(".stl") || lower.ends_with(".step") || lower.ends_with(".stp")) {
            continue;
        }
        let candidate = base_dir.join(&rel);
        let candidate_canon = match std::fs::canonicalize(&candidate) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !candidate_canon.starts_with(&base_canon) || !candidate_canon.is_file() {
            continue;
        }
        files.push(candidate_canon);
    }
    files
}

fn build_export_packet(watched_file: &str, exports: &[String]) -> Option<Vec<u8>> {
    let stl_files = collect_export_stl_files(watched_file, exports);
    if stl_files.is_empty() {
        return None;
    }

    let (format_id, filename, payload) = if stl_files.len() == 1 {
        let path = &stl_files[0];
        let data = std::fs::read(path).ok()?;
        let name = path.file_name()?.to_string_lossy().to_string();
        (1u8, name, data)
    } else {
        let mut zip_buf = std::io::Cursor::new(Vec::<u8>::new());
        {
            let mut zip = ZipWriter::new(&mut zip_buf);
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for path in &stl_files {
                let name = path.file_name()?.to_string_lossy().to_string();
                let data = std::fs::read(path).ok()?;
                if zip.start_file(name, options).is_err() {
                    return None;
                }
                if zip.write_all(&data).is_err() {
                    return None;
                }
            }
            if zip.finish().is_err() {
                return None;
            }
        }
        (2u8, "exports.zip".to_string(), zip_buf.into_inner())
    };

    let name_bytes = filename.as_bytes();
    let mut packet = Vec::with_capacity(8 + 1 + 4 + name_bytes.len() + 4 + payload.len());
    packet.extend_from_slice(b"EXPORT\0\0");
    packet.push(format_id);
    packet.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
    packet.extend_from_slice(name_bytes);
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(&payload);
    Some(packet)
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.mesh_tx.subscribe();
    let mut labels_rx = state.labels_tx.subscribe();
    let mut exports_rx = state.exports_tx.subscribe();

    // Send server info (file + branch + port)
    let current_exports = state.current_exports.read().await.clone();
    let info_json = serde_json::json!({
        "type": "info",
        "file": state.watched_file,
        "branch": state.git_branch,
        "port": state.port,
        "exports": current_exports
    });
    let _ = sender.send(Message::Text(info_json.to_string())).await;

    // Send current view config if available
    if let Some(view) = state.current_view.read().await.clone() {
        let _ = sender.send(Message::Binary(view.as_ref().to_vec())).await;
    }

    // Send current mesh if available
    if let Some(mesh) = state.current_mesh.read().await.clone() {
        let _ = sender.send(Message::Binary(mesh.as_ref().to_vec())).await;
    }

    // Send current field if available
    if let Some(field) = state.current_field.read().await.clone() {
        let _ = sender.send(Message::Binary(field.as_ref().to_vec())).await;
    }

    // Send current circuit if available
    if let Some(circuit) = state.current_circuit.read().await.clone() {
        let _ = sender.send(Message::Binary(circuit.as_ref().to_vec())).await;
    }

    // Send current NanoVNA if available
    if let Some(nanovna) = state.current_nanovna.read().await.clone() {
        let _ = sender.send(Message::Binary(nanovna.as_ref().to_vec())).await;
    }

    // Send current labels if available
    if let Some(labels) = state.current_labels.read().await.clone() {
        let _ = sender.send(Message::Text(labels)).await;
    }

    loop {
        tokio::select! {
            Ok(mesh) = rx.recv() => {
                if sender.send(Message::Binary(mesh.as_ref().to_vec())).await.is_err() {
                    break;
                }
            }
            Ok(labels) = labels_rx.recv() => {
                if sender.send(Message::Text(labels)).await.is_err() {
                    break;
                }
            }
            Ok(exports) = exports_rx.recv() => {
                if sender.send(Message::Text(exports)).await.is_err() {
                    break;
                }
            }
            Some(msg) = receiver.next() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                            if cmd.get("type").and_then(|v| v.as_str()) == Some("download_exports") {
                                let exports = state.current_exports.read().await.clone();
                                if let Some(packet) = build_export_packet(&state.watched_file, &exports) {
                                    if sender.send(Message::Binary(packet.into())).await.is_err() {
                                        break;
                                    }
                                }
                            } else if cmd.get("type").and_then(|v| v.as_str()) == Some("download_step") {
                                let watched = PathBuf::from(&state.watched_file);
                                let ext = watched
                                    .extension()
                                    .and_then(|s| s.to_str())
                                    .map(|s| s.to_ascii_lowercase())
                                    .unwrap_or_default();

                                // If we're literally viewing a .step/.stp file, just download it.
                                if ext == "step" || ext == "stp" {
                                    match occt_ir::export_step_packet_for_file(&watched) {
                                        Ok(packet) => {
                                            if sender.send(Message::Binary(packet.into())).await.is_err() {
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            let _ = state.labels_tx.send(
                                                serde_json::json!({
                                                    "type": "status",
                                                    "state": "error",
                                                    "detail": format!("STEP download failed: {}", e),
                                                })
                                                .to_string(),
                                            );
                                        }
                                    }
                                    continue;
                                }

                                let out_filename = watched
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .map(|s| format!("{}.step", s))
                                    .unwrap_or_else(|| "assembly.step".to_string());

                                let scene = state.current_scene.read().await.clone();
                                let Some(scene) = scene else {
                                    let _ = state.labels_tx.send(
                                        serde_json::json!({
                                            "type": "status",
                                            "state": "error",
                                            "detail": "No IR scene available for STEP export (try reloading the Lua file).",
                                        })
                                        .to_string(),
                                    );
                                    continue;
                                };

                                let _ = state.labels_tx.send(
                                    serde_json::json!({
                                        "type": "status",
                                        "state": "exporting",
                                        "detail": "Exporting STEP...",
                                    })
                                    .to_string(),
                                );

                                let packet = tokio::task::spawn_blocking(move || {
                                    occt_ir::export_step_packet_for_scene(&scene, &out_filename)
                                })
                                .await;

                                match packet {
                                    Ok(Ok(packet)) => {
                                        let _ = state.labels_tx.send(
                                            serde_json::json!({
                                                "type": "status",
                                                "state": "ready",
                                                "detail": "STEP export ready",
                                            })
                                            .to_string(),
                                        );
                                        if sender.send(Message::Binary(packet.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        let _ = state.labels_tx.send(
                                            serde_json::json!({
                                                "type": "status",
                                                "state": "error",
                                                "detail": format!("STEP export failed: {}", e),
                                            })
                                            .to_string(),
                                        );
                                    }
                                    Err(e) => {
                                        let _ = state.labels_tx.send(
                                            serde_json::json!({
                                                "type": "status",
                                                "state": "error",
                                                "detail": format!("STEP export task failed: {}", e),
                                            })
                                            .to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            else => break,
        }
    }
}
