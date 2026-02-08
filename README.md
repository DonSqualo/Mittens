# Mittens: Markdown for Reality

## Electronic Lab / Machine Shop / Human Body simulator for Agents and Keyboard Monkeys

![Mittens UI screenshot](screenshot-2026-01-06_14-11-05.png)

Mittens is a script-first CAD and multiphysics environment driven by Lua, a Rust backend, and a WebGPU renderer. It is built for rapid agent-assisted iteration on geometry, field studies, and instrumented experiments.

## CLI Quick Start (Current)

```bash
# From repo root
./mittens router start --registry "$HOME/.mittens/backends.json" --port 3100

./mittens backend start \
  --backend-id pure-acoustics \
  --project-file examples/pure_acoustics.lua \
  --projects-root "$PWD" \
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
- To use repo examples, pass `--projects-root "$PWD"` with `--project-file examples/...`.
- Example set is intentionally minimal: `examples/pure_acoustics.lua` and `examples/tube.lua`.
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

Label placement should use the `text(...)` primitive in scene scripts.

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
local Scene = require("stdlib")
local register = Scene.register
local serialize = Scene.serialize

Materials = {
  Copper = material("copper"),
  Polycarbonate = material("polycarbonate"),
}

Housing = {
  outer = { radius = 20, height = 30 },
  inner = { radius = 15, height = 30 },
}

Ring = {
  radius = { inner = 8, outer = 10 },
  height = 3,
  position = { x = 0, y = 0, z = 10 },
}

Probe = {
  position = { x = 0, y = 0, z = 20 },
  config = { range = "mT", label = "Bz Probe" },
}

Housing.model = difference(
  cylinder(Housing.outer.radius, Housing.outer.height):material(Materials.Polycarbonate),
  cylinder(Housing.inner.radius, Housing.inner.height)
):center("XY")

Ring.model = ring(Ring.radius.inner, Ring.radius.outer, Ring.height)
  :material(Materials.Copper)
  :at(Ring.position.x, Ring.position.y, Ring.position.z)

Assembly = group("assembly", { Housing.model, Ring.model })

register(Assembly)
register(GaussMeter(
  { Probe.position.x, Probe.position.y, Probe.position.z },
  Probe.config
))

view({
  flat_shading = false,
  camera = {
    position = {-80, -150, 80},
    target = {0, 0, 20},
  },
})

return serialize()
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
