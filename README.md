# Mittens 🐱

**Markdown for Reality: Electronic Lab / Machine Shop / Human Body simulator for Agents and Keyboard Monkeys**

Mittens is a script-first CAD and multiphysics environment driven by Lua, a Rust backend, and a WebGPU renderer. It is built for rapid agent-assisted iteration on geometry, field studies, and instrumented experiments.

## CLI Quick Start (Current)

```bash
# From repo root
./mittens router start --registry "$HOME/.mittens/backends.json" --port 3100

./mittens backend start \
  --backend-id pure-acoustics \
  --project-file /home/heim/projects/pure-acoustics/pure_acoustics.lua \
  --backend-port 4293 \
  --worktree "$PWD" \
  --registry "$HOME/.mittens/backends.json"

./mittens run renderer \
  --renderer-id local \
  --worktree "$PWD" \
  --port 3000 \
  --host 0.0.0.0
```

Open:

- `http://localhost:3000/?backend_id=pure-acoustics`

Notes:

- Router is the single `/ws/<backend_id>` entry point.
- `backend start` enforces project files under `~/projects` by default.
- Full command surface: `./mittens help`

## CLI Overview

- Router lifecycle:
  - `mittens router start|stop|status`
  - `mittens run router`
  - `mittens systemd router install|uninstall|start|stop|restart|status|logs`
- Backend lifecycle:
  - `mittens backend start|stop|status|list`
  - `mittens run backend`
  - `mittens systemd backend install|uninstall|start|stop|restart|status|logs`
- Renderer lifecycle:
  - `mittens run renderer`
  - `mittens systemd renderer install|uninstall|start|stop|restart|status|logs`
- Infra helpers:
  - `mittens nginx snippet`

## stdlib API (Current Surface)

The current public surface is exported in `stdlib/init.lua`.

### Core Scene API

- `local Mittens = require("stdlib")`
- `Mittens.register(obj)`
- `Mittens.serialize()`
- `Mittens.get_scene()`
- `Mittens.clear()`
- `Mittens.add_label(text, x, y, z, size?, color?)`
- `Mittens.clear_labels()`

### Global Geometry + Composition

- Primitives:
  - `box(w, d?, h?)`
  - `cylinder(r, h)`
  - `sphere(r)`
  - `torus(major_radius, minor_radius)`
  - `ring(inner_radius, outer_radius, height)`
  - `text(text_str, font_size?)`
- CSG:
  - `union(...)`
  - `difference(base, ...)`
  - `intersect(...)`
- Grouping:
  - `group(name?, children?)`
  - `assembly(name, children, metadata?)`
  - `component(name, children)`
- Materials:
  - `material(name, properties?)`

### Transforms + Math

- Functional transforms:
  - `translate(shape, x, y, z)`
  - `rotate(shape, rx, ry, rz)`
  - `scale(shape, sx, sy?, sz?)`
  - `mirror(shape, plane)`
  - `linear_pattern(shape, count, dx, dy?, dz?)`
  - `circular_pattern(shape, count, radius, axis?)`
- Math helpers:
  - `Vec3`
  - `Mat4`

### Physics Studies

- `magnetostatic(config?)`
- `acoustic(config?)`
- `acoustic_source(geometry, config?)`
- `acoustic_boundary(surface, config?)`
- `current_source(geometry, config?)`
- `linspace(start, stop, count)`
- `logspace(start, stop, count)`

### Instruments + Visualization

- `Probe(name, config?)`
- `GaussMeter(position, config?)`
- `MagneticFieldPlane(plane, offset, config?)`
- `AcousticPressurePlane(plane, offset, config?)`
- `Hydrophone(position, config?)`

### View + Export + Circuits

- View:
  - `view(config?)`
- Export:
  - `export_stl(filename, object, circular_segments?)`
  - `export_3mf(filename, object, config?)`
- Circuit helpers:
  - `SignalGenerator(config?)`
  - `Amplifier(config?)`
  - `MatchingNetwork(config?)`
  - `TransducerLoad(config?)`
  - `Circuit(config?)`

### Common Shape/Group Methods

Most shape/group objects support fluent methods:

- `:at(x, y, z)`
- `:rotate(rx, ry, rz)`
- `:scale(sx, sy?, sz?)`
- `:center("XY" | "XZ" | "YZ" | "XYZ")`
- `:material(mat)`
- `:color(r, g, b, a?)`
- `:name(str)`

## Minimal Example

```lua
local Mittens = require("stdlib")

local coil = ring(8, 10, 3):material(material("copper"))
local body = cylinder(20, 30):material(material("polycarbonate"))

local model = difference(body, cylinder(15, 30))
  :center("XY")
  :at(0, 0, 0)

Mittens.register(group("assembly", { model, coil:at(0, 0, 10) }))
Mittens.register(GaussMeter({0, 0, 20}, { range = "mT", label = "Bz Probe" }))
view({ flat_shading = false, camera = { position = {-80, -150, 80}, target = {0, 0, 20} } })

return Mittens.serialize()
```

## Project Structure

```text
stdlib/           Lua standard library
renderer/         WebGPU frontend (Vite + Three.js)
server/           Rust backend (Manifold CSG + studies + router bins)
examples/         Example scripts
```

## License

MIT
