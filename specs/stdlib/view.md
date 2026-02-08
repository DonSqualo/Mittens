# view.lua

Camera, visibility, and render configuration.

## view(config)

Main configuration function. Called without args returns current state.

```lua
view({camera = "isometric", flat_shading = true})
view()  -- returns View._state
```

### Camera Config

| Key | Type | Description |
|-----|------|-------------|
| camera | string/table | Preset name or {position, target, fov, ...} |
| distance | number | Scale camera distance from target |

**Presets:** `isometric`, `front`, `back`, `left`, `right`, `top`, `bottom`

```lua
view({camera = "front"})
view({camera = {position = {50, 50, 50}, target = {0, 0, 0}}})
view({camera = "isometric", distance = 200})
```

### Visibility

| Key | Type | Description |
|-----|------|-------------|
| show | table | Object names to show |
| hide | table | Object names to hide |

### Clipping

| Key | Type | Description |
|-----|------|-------------|
| clip | table | {plane = "XY"/"XZ"/"YZ", offset = n, show_caps = bool} |

### Render Settings

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| flat_shading | boolean | false | Sharp edge rendering |
| circular_segments | number | 32 | Curve resolution |

Top-level shortcuts work:
```lua
view({flat_shading = true})
-- equivalent to
view({render = {flat_shading = true}})
```

### Other Settings

| Key | Type | Description |
|-----|------|-------------|
| transparency | table | {object_name = alpha, ...} |
| theme | string | "dark" or "light" |
| grid | bool/table | Grid visibility or {show, size, divisions} |
| axes | bool/table | Axes visibility or {show, size} |

## Individual Functions

```lua
View.camera_position(x, y, z)
View.camera_target(x, y, z)
View.camera_fov(fov)
View.orthographic([scale])
View.perspective()
View.show("name1", "name2", ...)
View.hide("name1", "name2", ...)
View.clip("XZ", offset)
View.unclip()
View.set_transparency(object_or_name, alpha)
View.get_state()
View.reset()
View.load(state)
```

## Serialization

**Values serialized and sent to renderer:**
- `flat_shading`
- `circular_segments`
- `camera.position` (x, y, z)
- `camera.target` (x, y, z)
- `camera.fov`
- `camera.near`
- `camera.far`

Other view state (visibility, clipping) is NOT persisted via `View.serialize()`.

## Global Shortcut

`view` is exposed globally.

## Internal State

```lua
View._state = {
  camera = {
    position = {100, 100, 100},
    target = {0, 0, 0},
    up = {0, 0, 1},
    fov = 45,
    near = 0.1,
    far = 10000,
    projection = "perspective",
  },
  visible = {},
  hidden = {},
  clip = nil,
  transparency = {},
  theme = "dark",
  grid = {show = true, size = 100, divisions = 10},
  axes = {show = true, size = 20},
  render = {flat_shading = false, circular_segments = 32}
}
```
