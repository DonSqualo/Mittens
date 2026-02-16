# AGENT.md

Build and development notes for Claude agents.

## CRITICAL SERVE RULE (NON-NEGOTIABLE)

When the user says `SERVE X`, it always means:

1. Use the `mittens` CLI.
2. Resolve `X` to a Lua project file under `~/projects` (never from repo `examples/` unless the user explicitly asks for examples).
3. Serve that `~/projects` file via `mittens` using `--projects-root "$HOME/projects"` and the resolved `--project-file`.

Do not reinterpret `SERVE X` as anything else.

## Build Commands

### Server (Rust)
```bash
cd server
cargo test        # Run tests (60 tests across all modules)
cargo build       # Debug build
cargo build --release  # Release build
cargo run --release --bin ir_subagent -- --file ../examples/multiphysics/pure_acoustics.lua  # Canonical IR + mesh validation loop
cargo run --release --bin ir_subagent -- --file ../examples/multiphysics/pure_acoustics.lua --emit-baseline-stl --candidate-stl target/ir_subagent/baseline_manifold.stl  # Tolerance oracle + conformance self-check
```

### Renderer (TypeScript)
```bash
cd renderer
npm install       # Install dependencies
npm run build     # Production build to dist/
npm run dev       # Development server
```

## Running the Application

1. Start the server with a Lua file:
```bash
cd server
cargo run --release -- ../project/helmholtz_coil.lua
```

2. Serve the renderer (in another terminal):
```bash
cd renderer
npm run dev
```

3. Open browser to http://localhost:5173

The server watches the Lua file for changes and pushes updates via WebSocket to port 3001.

## Project Structure

- `stdlib/` - Lua standard library loaded by scripts
- `server/src/` - Rust backend (geometry, field computation, export)
- `renderer/src/` - TypeScript frontend (Three.js)
- `project/` - Current project Lua files
- `specs/` - Feature specifications and learnings

## Key Patterns

### Helmholtz Field Computation
The server pattern-matches for `helmholtz` or `coil_mean_radius` in Lua content to trigger Biot-Savart computation. Config is read from the `Coil` and `Wire` global tables (project convention).

### Binary Protocols
- Mesh data: `[num_vertices:u32][num_indices:u32][positions][normals][colors][indices]`
- Field data: Header `FIELD\0\0\0`, then slice dims, bounds, Bx, Bz, magnitude, arrows, line data
- View config: Header `VIEW\0\0\0\0`, then `flat_shading:u8`, optional camera data
- Circuit: Header `CIRCUIT\0`, then size, SVG data
- Measurement: Header `MEASURE\0`, then position, value, magnitude, label (GaussMeter/Hydrophone)
- Line probe: Header `LNPROBE\0`, then num_points, start, stop, positions, values, magnitudes, name
- NanoVNA: Header `NANOVNA\0`, then num_points, min_s11_db, min_s11_freq, frequencies, magnitudes, phases

## Test Files

- `server/src/export.rs` - 5 tests for STL/3MF export
- `server/src/field.rs` - 7 tests for magnetic field computation
- `server/src/acoustic.rs` - 8 tests for acoustic field computation
- `server/src/nanovna.rs` - 7 tests for NanoVNA S11 simulation
- `server/src/circuit.rs` - 18 tests for circuit SVG generation
- `server/src/geometry.rs` - 4 tests for mesh validation
