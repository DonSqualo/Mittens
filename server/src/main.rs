//! Mittens Server - Manifold CSG
//! - File watcher
//! - Lua parser
//! - Manifold mesh generation
//! - WebSocket binary mesh streaming

use anyhow::Result;
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
use std::{net::SocketAddr, path::PathBuf, sync::Arc, thread, time::Duration};
use tokio::sync::{broadcast, mpsc, RwLock};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

mod acoustic;
mod circuit;
mod export;
mod field;
mod geometry;
mod nanovna;

struct AppState {
    mesh_tx: broadcast::Sender<Vec<u8>>,
    labels_tx: broadcast::Sender<String>,
    current_mesh: RwLock<Option<Vec<u8>>>,
    current_field: RwLock<Option<Vec<u8>>>,
    current_circuit: RwLock<Option<Vec<u8>>>,
    current_nanovna: RwLock<Option<Vec<u8>>>,
    current_labels: RwLock<Option<String>>,
    watched_file: String,
    git_branch: String,
    port: u16,
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

    let (mesh_tx, _) = broadcast::channel::<Vec<u8>>(16);
    let (labels_tx, _) = broadcast::channel::<String>(16);
    let (lua_tx, lua_rx) = mpsc::unbounded_channel::<(String, PathBuf)>();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (labels_result_tx, mut labels_result_rx) = mpsc::unbounded_channel::<String>();

    // Lua processing thread (Manifold needs to run on same thread)
    let labels_tx_clone = labels_result_tx.clone();
    thread::spawn(move || {
        process_lua_files(lua_rx, result_tx, labels_tx_clone);
    });

    let state = Arc::new(AppState {
        mesh_tx: mesh_tx.clone(),
        labels_tx: labels_tx.clone(),
        current_mesh: RwLock::new(None),
        current_field: RwLock::new(None),
        current_circuit: RwLock::new(None),
        current_nanovna: RwLock::new(None),
        current_labels: RwLock::new(None),
        watched_file: file_path.display().to_string(),
        git_branch,
        port,
    });

    // Handle mesh/field/circuit/nanovna results
    let state_clone = state.clone();
    tokio::spawn(async move {
        while let Some(data) = result_rx.recv().await {
            let is_field = data.len() >= 5 && &data[0..5] == b"FIELD";
            let is_circuit = data.len() >= 8 && &data[0..8] == b"CIRCUIT\0";
            let is_nanovna = data.len() >= 8 && &data[0..8] == b"NANOVNA\0";

            if is_field {
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

    // Load initial file
    if file_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            let _ = lua_tx.send((content, file_path.clone()));
        }
    }

    // File watcher
    let lua_tx_clone = lua_tx.clone();
    let watch_path = file_path.clone();
    tokio::spawn(async move {
        watch_file(watch_path, lua_tx_clone).await;
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

fn serialize_view_config(flat_shading: bool, camera: Option<CameraState>) -> Vec<u8> {
    let mut data = Vec::with_capacity(48);
    data.extend_from_slice(b"VIEW\0\0\0\0");
    data.push(if flat_shading { 1 } else { 0 });

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

fn process_lua_files(mut rx: mpsc::UnboundedReceiver<(String, PathBuf)>, tx: mpsc::UnboundedSender<Vec<u8>>, labels_tx: mpsc::UnboundedSender<String>) {
    let lua = mlua::Lua::new();

    // Set up package path to include stdlib directory
    let package_path = lua.globals()
        .get::<_, mlua::Table>("package")
        .and_then(|p| p.get::<_, String>("path"))
        .unwrap_or_default();

    let stdlib_path = "../?.lua;../?/init.lua";
    let new_path = format!("{};{}", stdlib_path, package_path);

    if let Ok(package) = lua.globals().get::<_, mlua::Table>("package") {
        let _ = package.set("path", new_path);
    }

    while let Some((content, file_path)) = rx.blocking_recv() {
        let base_dir = file_path.parent().unwrap_or(std::path::Path::new("."));

        // Process mesh and exports
        match process_single_file(&lua, &content, base_dir) {
            Ok(result) => {
                // Send view config first
                let view_binary = serialize_view_config(result.flat_shading, result.camera);
                let _ = tx.send(view_binary);

                let binary = result.mesh.to_binary();
                info!(
                    "Generated mesh: {} vertices, {} triangles, {} bytes, flat_shading={}",
                    result.mesh.positions.len() / 3,
                    result.mesh.indices.len() / 3,
                    binary.len(),
                    result.flat_shading
                );
                let _ = tx.send(binary);

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
            Err(e) => error!("Lua error: {}", e),
        }

        // Try to compute magnetic field if this looks like a Helmholtz coil
        if let Some(field_data) = try_compute_helmholtz_field(&lua, &content) {
            let field_binary = field_data.to_binary();
            info!(
                "Generated field: {}x{} slice, {} arrows, {} line points, {} bytes",
                field_data.slice_width,
                field_data.slice_height,
                field_data.arrows_positions.len() / 3,
                field_data.line_z.len(),
                field_binary.len()
            );
            let _ = tx.send(field_binary);
        }

        // Try to compute acoustic field if this looks like an acoustic simulation
        if let Some(field_data) = try_compute_acoustic_field(&lua, &content) {
            let field_binary = field_data.to_binary();
            info!(
                "Generated acoustic field: {}x{} slice, {} bytes",
                field_data.slice_width,
                field_data.slice_height,
                field_binary.len()
            );
            let _ = tx.send(field_binary);
        }

        // Try to generate circuit diagram if this looks like a circuit definition
        if let Some(circuit_data) = try_generate_circuit(&lua, &content) {
            let circuit_binary = circuit_data.to_binary();
            info!(
                "Generated circuit: {}x{}, {} bytes SVG",
                circuit_data.width,
                circuit_data.height,
                circuit_data.svg.len()
            );
            let _ = tx.send(circuit_binary);
        }

        // Compute GaussMeter point measurements
        let gaussmeter_measurements = try_compute_gaussmeter_measurements(&lua, &content);
        for m in &gaussmeter_measurements {
            let binary = m.to_binary();
            let _ = tx.send(binary);
        }

        // Compute Probe line measurements
        let probe_measurements = try_compute_probe_measurements(&lua, &content);
        for m in &probe_measurements {
            let binary = m.to_binary();
            let _ = tx.send(binary);
        }

        // Compute Hydrophone point measurements and send to renderer
        let hydrophone_measurements = try_compute_hydrophone_measurements(&lua, &content);
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
            let _ = tx.send(binary);
        }

        // Compute NanoVNA frequency sweep if configured
        if let Some(sweep) = try_compute_nanovna_sweep(&lua, &content) {
            let sweep_binary = sweep.to_binary();
            info!(
                "Generated NanoVNA sweep: {} points, {} bytes",
                sweep.points.len(),
                sweep_binary.len()
            );
            let _ = tx.send(sweep_binary);
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
    #[allow(dead_code)]
    circular_segments: u32,
    camera: Option<CameraState>,
    labels: Vec<Label>,
}

#[derive(Clone, serde::Serialize)]
struct Label {
    text: String,
    x: f32,
    y: f32,
    z: f32,
    size: f32,
    color: String,
}

fn process_single_file(lua: &mlua::Lua, content: &str, base_dir: &std::path::Path) -> Result<ProcessResult> {
    // Clear scene state before each execution to prevent accumulation
    let _ = lua.load(r#"
        local loaded = package.loaded["stdlib"] or package.loaded["stdlib.init"]
        if loaded and loaded.clear then loaded.clear() end
    "#).exec();

    let result: mlua::Value = lua.load(content).eval()?;

    // Extract view config
    let (flat_shading, circular_segments, camera) = if let Some(table) = result.as_table() {
        if let Ok(view) = table.get::<_, mlua::Table>("view") {
            let flat = view.get::<_, bool>("flat_shading").unwrap_or(false);
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

            (flat, segments, cam)
        } else {
            (false, 32, None)
        }
    } else {
        (false, 32, None)
    };

    info!("Using Manifold backend for CSG, circular_segments={}", circular_segments);
    let mesh = geometry::generate_mesh_from_lua_manifold(lua, &result, circular_segments)?;

    if let Some(table) = result.as_table() {
        export::process_exports_from_table(lua, table, base_dir);
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
                    labels.push(Label { text, x, y, z, size, color });
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

    Ok(ProcessResult { mesh, flat_shading, circular_segments, camera, labels })
}

async fn watch_file(path: PathBuf, tx: mpsc::UnboundedSender<(String, PathBuf)>) {
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
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                info!("File changed, regenerating mesh...");
                let _ = tx.send((content, path.clone()));
            }
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.mesh_tx.subscribe();
    let mut labels_rx = state.labels_tx.subscribe();

    // Send server info (file + branch + port)
    let info_json = serde_json::json!({
        "type": "info",
        "file": state.watched_file,
        "branch": state.git_branch,
        "port": state.port
    });
    let _ = sender.send(Message::Text(info_json.to_string())).await;

    // Send current mesh if available
    if let Some(mesh) = state.current_mesh.read().await.clone() {
        let _ = sender.send(Message::Binary(mesh.into())).await;
    }

    // Send current field if available
    if let Some(field) = state.current_field.read().await.clone() {
        let _ = sender.send(Message::Binary(field.into())).await;
    }

    // Send current circuit if available
    if let Some(circuit) = state.current_circuit.read().await.clone() {
        let _ = sender.send(Message::Binary(circuit.into())).await;
    }

    // Send current NanoVNA if available
    if let Some(nanovna) = state.current_nanovna.read().await.clone() {
        let _ = sender.send(Message::Binary(nanovna.into())).await;
    }

    // Send current labels if available
    if let Some(labels) = state.current_labels.read().await.clone() {
        let _ = sender.send(Message::Text(labels)).await;
    }

    loop {
        tokio::select! {
            Ok(mesh) = rx.recv() => {
                if sender.send(Message::Binary(mesh.into())).await.is_err() {
                    break;
                }
            }
            Ok(labels) = labels_rx.recv() => {
                if sender.send(Message::Text(labels)).await.is_err() {
                    break;
                }
            }
            Some(msg) = receiver.next() => {
                match msg {
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            else => break,
        }
    }
}
