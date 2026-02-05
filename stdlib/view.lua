-- Mittens Standard Library: View
-- Camera, visibility, and rendering configuration

local View = {}

-- Current view state
View._state = {
  camera = {
    position = {-80, -150, 80},  -- Y axis into screen, Z up
    target = {0, 0, 20},
    up = {0, 0, 1},
    fov = 45,
    near = 0.1,
    far = 10000,
    projection = "perspective",  -- perspective, orthographic
  },
  visible = {},
  hidden = {},
  clip = nil,
  transparency = {},
  theme = "dark",
  grid = {
    show = true,
    size = 100,
    divisions = 10,
  },
  axes = {
    show = true,
    size = 20,
  },
  render = {
    flat_shading = false,
    circular_segments = 32,
  }
}

--- Configure the view
-- @param config View configuration table
function View.view(config)
  if not config then return View._state end

  -- Apply camera position
  if config.camera then
    for k, v in pairs(config.camera) do
      View._state.camera[k] = v
    end
  end

  -- Camera distance (for presets)
  if config.distance then
    local dir = {
      View._state.camera.position[1] - View._state.camera.target[1],
      View._state.camera.position[2] - View._state.camera.target[2],
      View._state.camera.position[3] - View._state.camera.target[3],
    }
    local len = math.sqrt(dir[1]^2 + dir[2]^2 + dir[3]^2)
    local scale = config.distance / len
    View._state.camera.position = {
      View._state.camera.target[1] + dir[1] * scale,
      View._state.camera.target[2] + dir[2] * scale,
      View._state.camera.target[3] + dir[3] * scale,
    }
  end

  -- Visible objects
  if config.show then
    View._state.visible = config.show
  end

  -- Hidden objects
  if config.hide then
    View._state.hidden = config.hide
  end

  -- Clipping plane
  if config.clip then
    View._state.clip = {
      plane = config.clip.plane or "XZ",
      offset = config.clip.offset or 0,
      show_caps = config.clip.show_caps ~= false,
    }
  end

  -- Transparency overrides
  if config.transparency then
    View._state.transparency = config.transparency
  end

  -- Theme
  if config.theme then
    View._state.theme = config.theme
  end

  -- Grid settings
  if config.grid ~= nil then
    if type(config.grid) == "boolean" then
      View._state.grid.show = config.grid
    else
      for k, v in pairs(config.grid) do
        View._state.grid[k] = v
      end
    end
  end

  -- Axes settings
  if config.axes ~= nil then
    if type(config.axes) == "boolean" then
      View._state.axes.show = config.axes
    else
      for k, v in pairs(config.axes) do
        View._state.axes[k] = v
      end
    end
  end

  -- Render settings
  if config.render then
    for k, v in pairs(config.render) do
      View._state.render[k] = v
    end
  end

  -- Top-level shortcuts
  if config.flat_shading ~= nil then
    View._state.render.flat_shading = config.flat_shading
  end
  if config.circular_segments ~= nil then
    View._state.render.circular_segments = config.circular_segments
  end

  return View._state
end

--- Set camera position
-- @param x X position
-- @param y Y position
-- @param z Z position
function View.camera_position(x, y, z)
  View._state.camera.position = {x, y, z}
end

--- Set camera target (look-at point)
-- @param x X target
-- @param y Y target
-- @param z Z target
function View.camera_target(x, y, z)
  View._state.camera.target = {x, y, z}
end

--- Set field of view
-- @param fov FOV in degrees
function View.camera_fov(fov)
  View._state.camera.fov = fov
end

--- Use orthographic projection
-- @param scale Scale factor (optional)
function View.orthographic(scale)
  View._state.camera.projection = "orthographic"
  if scale then
    View._state.camera.ortho_scale = scale
  end
end

--- Use perspective projection
function View.perspective()
  View._state.camera.projection = "perspective"
end

--- Show specific objects
-- @param ... Object names to show
function View.show(...)
  local names = {...}
  for _, name in ipairs(names) do
    table.insert(View._state.visible, name)
    -- Remove from hidden if present
    for i, h in ipairs(View._state.hidden) do
      if h == name then
        table.remove(View._state.hidden, i)
        break
      end
    end
  end
end

--- Hide specific objects
-- @param ... Object names to hide
function View.hide(...)
  local names = {...}
  for _, name in ipairs(names) do
    table.insert(View._state.hidden, name)
    -- Remove from visible if present
    for i, v in ipairs(View._state.visible) do
      if v == name then
        table.remove(View._state.visible, i)
        break
      end
    end
  end
end

--- Set clipping plane
-- @param plane "XY", "XZ", or "YZ"
-- @param offset Distance from origin
function View.clip(plane, offset)
  View._state.clip = {
    plane = plane,
    offset = offset or 0,
    show_caps = true,
  }
end

--- Remove clipping plane
function View.unclip()
  View._state.clip = nil
end

--- Set object transparency
-- @param object Object or object name
-- @param alpha Transparency (0-1)
function View.set_transparency(object, alpha)
  local name = type(object) == "string" and object or object._name
  View._state.transparency[name] = alpha
end

--- Get current view state
-- @return View state table
function View.get_state()
  return View._state
end

--- Serialize view state for saving/transmission
-- @return Serialized view state
function View.serialize()
  return {
    flat_shading = View._state.render.flat_shading,
    circular_segments = View._state.render.circular_segments,
    camera = {
      position = View._state.camera.position,
      target = View._state.camera.target,
      fov = View._state.camera.fov,
      near = View._state.camera.near,
      far = View._state.camera.far,
    },
  }
end

--- Load view state
-- @param state Serialized state
function View.load(state)
  for k, v in pairs(state) do
    View._state[k] = v
  end
end

--- Reset to default view
function View.reset()
  View._state = {
    camera = {
      position = {-80, -150, 80},
      target = {0, 0, 20},
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
end

-- Global shortcut
view = View.view

return View
