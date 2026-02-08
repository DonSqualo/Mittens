# Mittens Architecture Overview

For contributors working on this codebase.

## Philosophy

This is a CAD tool with one fundamental philosophy: for each project (real world use case), keep the core minimal and push project-specific complexity to scripts. Successful projects add `/specs` notes with learnings and pitfalls so future work starts from stronger baselines.

**Core tenets:**
- stdlib contains only essential primitives and pipeline: `box`, `cylinder`, `sphere`, `torus`, `union`, `difference`, `group`, `assembly`, `component`, `view`, `export_stl`, `export_3mf`
- Project-specific primitives and workflows can be added per project when needed
- The renderer has no UI - all configuration is done in Lua
- All project code is effectively garbage collected after use
- "Better" means faster iteration and more documented learnings, NOT more preexisting code
- Avoid speculative framework growth; keep reusable core additions intentional
- Specs capture learnings and pitfalls, not reusable code

See [garbage_collection.md](server/garbage_collection.md) for the post-project review process.
See [implementation_status.md](server/implementation_status.md) for API vs backend implementation matrix.

## Documentation Philosophy

When writing docs (Rust `///` comments, Lua `--` comments, spec files):
- Capture WHY tests and implementations matter, not just what they do
- Link to real-world purpose: what device does this simulate? What measurement does this enable?
- No aspirational docs - if backend doesn't exist, say "API only" explicitly
- Cross-reference implementation_status.md for current state

## Pipeline

```
Lua Script (.lua)
      |
      v
+------------------+
|   Lua Runtime    |  stdlib: primitives, csg, groups, export, view
|   (mlua)         |  Shapes stored as SDF + operation list
+------------------+
      |
      | JSON serialization
      v
+------------------+
|   Rust Backend   |  geometry.rs: SDF -> mesh via Manifold
|   (Axum server)  |  export.rs: STL/STEP/3MF generation
+------------------+
      |
      | WebSocket: mesh data + view state
      v
+------------------+
|   Renderer       |  Three.js web renderer
|   (TypeScript)   |  Receives 3MF-equivalent data
+------------------+
```

## Data Flow

1. User writes Lua script defining shapes via `box()`, `cylinder()`, CSG ops
2. Shapes accumulate transforms via method chaining (`:at()`, `:rotate()`)
3. `view()` configures camera, visibility, render settings
4. `export_stl()` etc. queue export jobs
5. Script execution returns JSON: `{objects: [...], view: {...}, exports: [...]}`
6. Rust converts SDF descriptions to triangle meshes via Manifold library
7. Meshes sent to renderer via WebSocket
8. Export queue processed to write files

## Key Files

| File | Purpose |
|------|---------|
| `stdlib/primitives.lua` | `box()`, `cylinder()` |
| `stdlib/csg.lua` | `union()`, `difference()` |
| `stdlib/groups.lua` | `group()`, `assembly()`, `component()` |
| `stdlib/export.lua` | `export_stl()`, `export_3mf()` |
| `stdlib/view.lua` | `view()`, camera presets, clipping |
| `server/src/geometry.rs` | SDF to mesh conversion |
| `server/src/export.rs` | File format writers |
| `renderer/src/main.ts` | Three.js scene management |

## Coordinate System

- Right-handed: +X right, +Y forward, +Z up
- Units: millimeters (mm) by default
- Rotation: degrees, applied as Euler angles (X, Y, Z order)

## Shape Representation

Shapes carry:
- `_sdf`: Signed distance function for evaluation
- `_bounds`: Axis-aligned bounding box
- `_ops`: List of transforms to apply
- `_metadata`: Primitive type and params for serialization

Transforms are NOT baked into SDF - they're serialized and applied by Rust backend.

## Module Index

- [primitives.md](stdlib/primitives.md) - Basic shapes
- [csg.md](stdlib/csg.md) - Boolean operations
- [groups.md](stdlib/groups.md) - Hierarchy and components
- [export.md](stdlib/export.md) - File output
- [view.md](stdlib/view.md) - Camera and rendering
- [gotchas.md](stdlib/gotchas.md) - Known pitfalls
