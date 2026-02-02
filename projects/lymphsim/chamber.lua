-- LymphSim: Bath Chamber Geometry
-- 2m × 0.6m rectangular bath with speakers at each end

local Chamber = {}

-- Bath dimensions (meters)
Chamber.config = {
  length = 2.0,        -- X: bath length
  width = 0.6,         -- Y: bath width  
  depth = 0.4,         -- Z: water depth
  wall_thickness = 0.02,
  water_level = 0.35,  -- Z: water surface
  speaker_radius = 0.08,
  speaker_depth = 0.05,
}

--- Create the bath shell (outer walls minus inner cavity)
function Chamber.create_bath()
  local c = Chamber.config
  
  -- Outer shell
  local outer = box(
    c.length + 2 * c.wall_thickness,
    c.width + 2 * c.wall_thickness,
    c.depth + c.wall_thickness
  ):color(0.7, 0.7, 0.75, 1.0):name("bath_outer")
  
  -- Inner cavity (cut out)
  local inner = box(c.length, c.width, c.depth + 0.01)
    :at(c.wall_thickness, c.wall_thickness, c.wall_thickness)
  
  local bath_shell = difference(outer, inner)
    :name("bath_shell")
    :color(0.8, 0.82, 0.85, 1.0)
  
  return bath_shell
end

--- Create the water volume (semi-transparent blue)
function Chamber.create_water()
  local c = Chamber.config
  
  local water = box(c.length - 0.01, c.width - 0.01, c.water_level)
    :at(c.wall_thickness + 0.005, c.wall_thickness + 0.005, c.wall_thickness)
    :color(0.2, 0.4, 0.8, 0.3)
    :name("water")
    :material(material("water"))
  
  return water
end

--- Create speakers at both ends of the bath
function Chamber.create_speakers()
  local c = Chamber.config
  
  -- Speaker height (center of water volume)
  local speaker_z = c.wall_thickness + c.water_level / 2
  local speaker_y = c.wall_thickness + c.width / 2
  
  -- Left speaker (X = 0 end)
  local speaker_left = cylinder(c.speaker_radius, c.speaker_depth)
    :rotate(0, 90, 0)  -- Point along X axis
    :at(0, speaker_y, speaker_z)
    :color(0.3, 0.3, 0.35, 1.0)
    :name("speaker_left")
    :material(material("pzt"))
  
  -- Right speaker (X = length end)
  local speaker_right = cylinder(c.speaker_radius, c.speaker_depth)
    :rotate(0, -90, 0)  -- Point along -X axis
    :at(c.length + 2 * c.wall_thickness, speaker_y, speaker_z)
    :color(0.3, 0.3, 0.35, 1.0)
    :name("speaker_right")
    :material(material("pzt"))
  
  return group("speakers", {speaker_left, speaker_right})
end

--- Create the complete chamber assembly
function Chamber.create()
  local c = Chamber.config
  
  local chamber = group("chamber", {
    Chamber.create_bath(),
    Chamber.create_water(),
    Chamber.create_speakers(),
  })
  
  return chamber
end

-- Export dimensions for other modules
Chamber.center_x = Chamber.config.wall_thickness + Chamber.config.length / 2
Chamber.center_y = Chamber.config.wall_thickness + Chamber.config.width / 2
Chamber.water_surface_z = Chamber.config.wall_thickness + Chamber.config.water_level

return Chamber
