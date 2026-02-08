# csg.lua

Constructive Solid Geometry operations.

## Pipeline

```
Lua (stdlib/csg.lua)
         ↓
   JSON serialization
         ↓
┌────────────────────────────────────────────┐
│           Rust Backend                      │
│           manifold3d                        │
│           geometry.rs                       │
└────────────────────────────────────────────┘
         ↓
   Watertight mesh
```

## Supported Operations

| Operation | Lua | Backend |
|-----------|-----|---------|
| `union` | ✓ | ✓ |
| `difference` | ✓ | ✓ |
| `intersect` | ✓ | ✓ |

## Lua API

All CSG results support: `at`, `rotate`, `scale`, `center`, `material`, `color`, `name`, `eval`, `serialize`.

### union(...)

Combines shapes additively. Flattens arrays.

```lua
union(box(10), cylinder(5, 20))
union({box(10), box(10):at(5, 0, 0)})  -- array flattened
union(box(10))  -- returns box unchanged
```

**Bounds:** Union of all child bounds.

### difference(base, ...)

Subtracts subsequent shapes from base. Flattens arrays.

```lua
difference(box(20), cylinder(5, 20):at(10, 10, 0))
difference(base, {hole1, hole2})  -- array flattened
difference(box(10))  -- returns box unchanged
```

**Bounds:** Same as base shape.

### intersect(...)

Returns only the volume common to all shapes. Flattens arrays.

```lua
intersect(box(10), cylinder(5, 20))
intersect({sphere1, sphere2, sphere3})  -- array flattened
intersect(box(10))  -- returns box unchanged
```

**Bounds:** Overlap region of all child bounds (may be empty).

## Serialization

```lua
{
  type = "csg",
  operation = "union" | "difference" | "intersect",
  children = [...],
  ops = [...],
  material = ...,
  color = {...},
  name = ...
}
```

## Backend Details

Location: `server/src/geometry.rs`

Uses `manifold3d` for guaranteed watertight manifolds.

```rust
result = match operation.as_str() {
    "union" => result.union(&child_manifold),
    "difference" => result.difference(&child_manifold),
    "intersect" => result.intersection(&child_manifold),
    _ => ...
};
```

## SDF Functions (Lua only)

For local preview/bounds, not mesh generation:
- `union`: `min(d1, d2, ...)`
- `difference`: `max(d_base, -d_cutter1, ...)`
- `intersect`: `max(d1, d2, ...)`
