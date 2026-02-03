# Lua View/Camera API

## Setting the Camera

The camera is configured through the `view` table in the script's return value.

### Syntax

```lua
return {
  -- Your geometry here (mesh, unions, etc.)
  my_geometry,
  
  -- View configuration
  view = {
    camera = {
      position = {x, y, z},  -- Camera position in world coordinates
      target = {x, y, z},    -- Point the camera looks at
      up = {x, y, z},        -- Up vector (typically {0, 0, 1} for Z-up)
      fov = 45               -- Field of view in degrees (optional, default 45)
    },
    flat_shading = false,    -- Use flat shading instead of smooth (optional)
    circular_segments = 32   -- Segments for circular geometry (optional)
  }
}
```

### Example

```lua
-- Create some geometry
local sphere = Sphere(50)
local box = Box(100, 100, 100):translate(0, 0, 60)
local scene = sphere + box

-- Return with camera view
return {
  scene,
  view = {
    camera = {
      position = {-300, -200, 150},
      target = {0, 0, 50},
      up = {0, 0, 1}
    }
  }
}
```

### Common Mistakes

❌ **Wrong:** Standalone view statement
```lua
view camera = { position = {0,0,0} }  -- This is invalid Lua syntax
```

❌ **Wrong:** View outside return table
```lua
local view = { camera = {...} }  -- Won't be seen by renderer
return my_geometry
```

✅ **Correct:** View inside return table
```lua
return {
  my_geometry,
  view = { camera = {...} }
}
```

### Coordinate System

Mittens uses a **Z-up** coordinate system:
- X: right
- Y: forward  
- Z: up

Camera `up` vector should typically be `{0, 0, 1}`.

### Notes

- Camera settings are sent to the renderer via WebSocket binary protocol
- If no camera is specified, renderer uses default position `(-80, -150, 80)`
- The camera info widget (top-right) shows current position and target
