# groups.lua

Hierarchical organization of shapes.

## group(name, children) or group(children)

Creates a named container for shapes/groups.

| Param | Type | Description |
|-------|------|-------------|
| name | string | Group name (optional, defaults to "unnamed_group") |
| children | table | Array of shapes/groups |

```lua
group("fasteners", {screw1, screw2, nut})
group({shape1, shape2})  -- unnamed
```

### Group Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `at` | `(x, y, z)` | Translate group |
| `rotate` | `(rx, ry, rz)` | Rotate group |
| `scale` | `(sx, [sy], [sz])` | Scale group |
| `center` | `("X" | "Y" | "Z" | "XY" | "XZ" | "YZ" | "XYZ")` | Center on selected axes |
| `material` | `(mat)` | Set material for group |
| `color` | `(r, g, b, [a])` | Set color for group |
| `hide` | `()` | Set invisible |
| `show` | `()` | Set visible |
| `lock` | `()` | Prevent editing |
| `unlock` | `()` | Allow editing |
| `add` | `(child)` | Add child (updates bounds) |
| `remove` | `(child_or_name)` | Remove child (bounds recalculated) |
| `find` | `(name)` | Recursive search for named child |
| `flatten` | `()` | Get all non-group descendants |
| `serialize` | `()` | Convert to JSON |

**Validation:**
```lua
local g = group("test", {box(10), box(10):at(20, 0, 0)})
g:find("box")  -- nil (shapes unnamed by default)
#g:flatten()   -- 2
```

## assembly(name, children, [metadata])

Top-level group with metadata. Adds `_metadata.created` timestamp.

```lua
assembly("my_device", {part1, part2}, {author = "user", version = "1.0"})
```

## component(name, children)

Reusable part with instancing support.

```lua
local bolt = component("M3_bolt", {head, shaft})
local inst1 = bolt:instance():at(0, 0, 0)
local inst2 = bolt:instance():at(10, 0, 0)
```

### Instance Methods

Instances only support transforms:
- `at(x, y, z)`
- `rotate(rx, ry, rz)`
- `scale(sx, [sy], [sz])`
- `serialize()`

Instances serialize as `{type = "instance", component = "name", ops = [...]}`

## Global Shortcuts

These are exposed globally (potential namespace collision):
- `group()`
- `assembly()`
- `component()`

## Internal Structure

```lua
grp = {
  _type = "group" | "assembly" | "component",
  _name = "...",
  _children = [...],
  _ops = [],
  _visible = true,
  _locked = false,
  _bounds = {min = {...}, max = {...}},
  _metadata = {},     -- assembly only
  _instances = [],    -- component only
}
```
