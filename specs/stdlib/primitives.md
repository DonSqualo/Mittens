# primitives.lua

Basic geometric shapes.

## Shape Object

All primitives return a Shape with these methods:

| Method | Signature | Description |
|--------|-----------|-------------|
| `at` | `(x, y, z)` | Translate shape |
| `rotate` | `(rx, ry, rz)` | Rotate in degrees |
| `scale` | `(sx, [sy], [sz])` | Scale (uniform if 1 arg) |
| `center` | `("X" | "Y" | "Z" | "XY" | "XZ" | "YZ" | "XYZ")` | Center on selected axes |
| `material` | `(mat)` | Set material name |
| `color` | `(r, g, b, [a])` | Set RGBA color (0-1) |
| `name` | `(n)` | Set object name |
| `eval` | `(x, y, z)` | Evaluate SDF at point |
| `serialize` | `()` | Convert to JSON-serializable table |

## Primitives

### box(w, [d], [h])

Creates a box with corner at origin, extending to +w, +d, +h.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| w | number | required | Width (X) |
| d | number | w | Depth (Y) |
| h | number | w | Height (Z) |

```lua
box(10)              -- 10x10x10 cube
box(20, 10, 5)       -- 20x10x5 box
box(10):center("XYZ")   -- Centered at origin
```

**Validation:** `box(10)` creates bounds `{min={0,0,0}, max={10,10,10}}`

### cylinder(r, h)

Creates a cylinder with base on XY plane at Z=0, extending to +h.

| Param | Type | Description |
|-------|------|-------------|
| r | number | Radius |
| h | number | Height |

```lua
cylinder(5, 20)              -- r=5, h=20
cylinder(5, 20):center("XYZ")   -- Center at origin
```

**Validation:** `cylinder(5, 20)` creates bounds `{min={-5,-5,0}, max={5,5,20}}`

### sphere(r)

Creates a sphere centered at origin. Used for modeling spherical samples, cavities, or electromagnetic field sources.

| Param | Type | Description |
|-------|------|-------------|
| r | number | Radius (mm) |

```lua
sphere(10)              -- r=10mm sphere at origin
sphere(5):at(0, 0, 20)  -- Translated sphere
```

**Backend:** `geometry.rs` uses `Manifold::new_sphere`

**Validation:** `sphere(10)` creates bounds `{min={-10,-10,-10}, max={10,10,10}}`

### torus(major_radius, minor_radius)

Creates a torus centered at origin, lying in the XY plane. Used for modeling toroidal inductors, RF coils, and magnetic field simulations around circular conductors.

| Param | Type | Description |
|-------|------|-------------|
| major_radius | number | Distance from torus center to tube center (mm) |
| minor_radius | number | Radius of the tube cross-section (mm) |

```lua
torus(20, 5)              -- Donut: 20mm major, 5mm minor radius
torus(15, 3):rotate(90, 0, 0)  -- Rotated to YZ plane
```

**Backend:** `geometry.rs` uses parametric mesh generation

**Validation:** `torus(20, 5)` creates bounds `{min={-25,-25,-5}, max={25,25,5}}`

### ring(inner_radius, outer_radius, height)

Creates a ring (hollow cylinder/tube) centered at origin on XY plane, extending from Z=0 to +height. Used for modeling cylindrical housings, magnet holders, and annular components.

| Param | Type | Description |
|-------|------|-------------|
| inner_radius | number | Inner radius (mm) |
| outer_radius | number | Outer radius (mm) |
| height | number | Height (mm) |

```lua
ring(8, 10, 15)           -- Tube: 8mm inner, 10mm outer, 15mm tall
ring(5, 8, 20):center("XYZ") -- Centered ring
```

**Backend:** `geometry.rs` implements as difference of two cylinders

**Validation:** `ring(8, 10, 15)` creates bounds `{min={-10,-10,0}, max={10,10,15}}`

## Internal Structure

```lua
shape = {
  _type = "shape",
  _sdf = function(x, y, z) ... end,
  _bounds = {min = {x, y, z}, max = {x, y, z}},
  _ops = {},      -- transforms added via method calls
  _material = nil,
  _color = nil,
  _name = nil,
  _metadata = {primitive = "box", params = {w=10, d=10, h=10}}
}
```
