# Architecture

Technical details for contributors working on this codebase.

## System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              User                                        │
│                         (Neovim / Editor)                                │
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │ .lua files
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      Rust Server (mittens-server)                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │ File        │  │ Lua         │  │ Geometry    │  │ WebSocket   │    │
│  │ Watcher     │─▶│ Runner      │─▶│ (manifold)  │─▶│ Server      │    │
│  │ (notify)    │  │ (mlua)      │  │             │  │ (axum)      │    │
│  └─────────────┘  └─────────────┘  └─────────────┘  └──────┬──────┘    │
└─────────────────────────────────────────────────────────────┼───────────┘
                                                              │ JSON/WS
                                                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      Renderer (Web Browser / WebView)                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                      │
│  │ Three.js    │  │ Camera      │  │ Mesh        │                      │
│  │ Scene       │─▶│ Controls    │─▶│ Rendering   │                      │
│  └─────────────┘  └─────────────┘  └─────────────┘                      │
└─────────────────────────────────────────────────────────────────────────┘
```

## Data Flow

```
1. User edits model.lua
2. User saves file
3. notify detects file change
4. Server reads and executes Lua script
5. Script returns scene data (objects, view, exports)
6. Server converts SDF descriptions to meshes via manifold3d
7. WebSocket broadcasts mesh data to renderer
8. Three.js renders meshes
9. Export queue processed (STL/3MF files written)
```

## Rust Server Components

| Component | Crate | Purpose |
|-----------|-------|---------|
| HTTP/WS | axum | WebSocket server |
| Lua | mlua | Lua 5.4 interpreter |
| File watch | notify | Detect .lua changes |
| Geometry | manifold3d | SDF to mesh, CSG ops |

**Key files:**
- `server/src/main.rs` - Server entry, WS handling
- `server/src/geometry.rs` - SDF to mesh conversion
- `server/src/export.rs` - STL/3MF writers

## Lua Execution

Scripts execute in isolated Lua 5.4 environment with stdlib preloaded.

**Return format:**
```lua
{
  objects = {...},      -- serialized shapes/groups
  instruments = {...},  -- measurement visualizations
  studies = {...},      -- physics studies
  exports = {...},      -- queued file exports
  view = {...},         -- render settings
}
```

## WebSocket Protocol

```json
// Server → Client: scene update
{
  "type": "scene_update",
  "payload": { "objects": [...], "view": {...} },
  "filename": "model.lua"
}

// Server → Client: error
{
  "type": "compile_error",
  "payload": { "message": "...", "line": 42 }
}

// Client → Server: load file
{
  "type": "load_file",
  "path": "/path/to/model.lua"
}
```

## Renderer

Three.js in a browser/WebView. Receives triangle meshes (not SDFs).

**Responsibilities:**
- Mesh rendering with materials/colors
- Camera controls (orbit, pan, zoom)
- Grid and axes display
- Field visualization planes

**Does NOT do:**
- Geometry computation (server handles this)
- Physics simulation
- File I/O

## Extension Points

**Custom primitives:** Generate in project Lua, not stdlib. See specs for formulas.

**Physics:** Add to `stdlib/physics.lua` only if foundational. Project-specific sims go in project files.

**Export formats:** Add handlers to `server/src/export.rs`.
