# Gotchas

Known pitfalls for contributors working on this codebase.

## Coordinate Origins

**Box vs Cylinder inconsistency:**
- `box(10)` has corner at origin (0,0,0), extends to (10,10,10)
- `cylinder(5, 20)` is centered on XY (bounds -5 to +5), but Z starts at 0

```lua
-- To center both at origin:
box(10):center("XYZ")       -- now at (-5,-5,-5) to (5,5,5)
cylinder(5, 20):center("XYZ")  -- now at (-5,-5,-10) to (5,5,10)
```

## Transform Order

Transforms are applied in call order. Order matters.

```lua
box(10):at(20, 0, 0):rotate(0, 0, 45)  -- translate THEN rotate
box(10):rotate(0, 0, 45):at(20, 0, 0)  -- rotate THEN translate (different result)
```

## CSG Array Flattening

`union` and `difference` flatten nested arrays:
```lua
union(a, {b, c})  -- becomes union(a, b, c)
difference(base, {hole1, hole2})  -- becomes difference(base, hole1, hole2)
```

## Single-Shape CSG

Passing single shape to `union` returns it unwrapped:
```lua
local u = union(box(10))
u._type  -- "shape", NOT "csg"
```

## Group Bounds After Remove

`group:add(child)` and `group:remove(child)` both recalculate bounds.

```lua
local g = group({box(10), box(10):at(100, 0, 0)})
-- bounds: 0 to 110
g:remove(g._children[2])
-- bounds now 0 to 10
```

## View Serialization

`View.serialize()` only returns `flat_shading` and `circular_segments`. Camera position, visibility, clipping, theme are NOT serialized.

```lua
View.serialize()
-- returns: {flat_shading = false, circular_segments = 32}
-- camera, hidden, clip, theme are LOST
```

## Component Instance References

Instances reference components by name string, not object:
```lua
local bolt = component("M3_bolt", {...})
local inst = bolt:instance()
inst._component  -- "M3_bolt" (string)
```

If component name changes, instances break.

## Global Namespace Pollution

These are exposed globally and can collide with user variables:
- `group`, `assembly`, `component`
- `view`
- `export_stl`, `export_3mf`

## Export Queue vs Immediate

Exports are queued, not executed immediately:
```lua
export_stl("file.stl", shape)
-- file.stl does NOT exist yet
-- queue is processed by Rust backend after script completes
```

## Default Values

- `box(w)` defaults d=w, h=w (cube)
- `scale(s)` defaults sy=sx, sz=sx (uniform)
- `color(r, g, b)` defaults a=1.0
- `export_stl` defaults circular_segments=128
- View defaults circular_segments=32 (lower than export)

## Rotation Units

All rotations are in DEGREES, not radians:
```lua
shape:rotate(0, 0, 90)  -- 90 degrees, not pi/2
```

## SDF Evaluation vs Rendering

`shape:eval(x, y, z)` evaluates the SDF but ignores `_ops` transforms. The SDF is for the original shape. Transforms are applied during mesh generation by Rust.
