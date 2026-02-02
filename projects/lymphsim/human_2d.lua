-- LymphSim: Human Cross-Section (2D Slice)
-- Simplified elliptical body outline for visualization

local Human = {}

-- Human body dimensions (cross-section, meters)
Human.config = {
  body_width = 0.35,     -- Y: shoulder width (diameter)
  body_depth = 0.20,     -- Z: front-to-back (diameter)
  body_length = 0.04,    -- X: slice thickness (thin for 2D visualization)
  head_radius = 0.08,    -- Head circle radius
  head_offset_z = 0.12,  -- Head center above body center
}

--- Create simplified 2D elliptical body cross-section
-- Uses a scaled cylinder to approximate ellipse extruded along X
function Human.create_body()
  local c = Human.config
  
  -- Create a cylinder, then scale to make ellipse
  -- Cylinder base radius = 1, then scale Y and Z differently
  local base_radius = 1.0
  
  -- Body ellipse (scaled cylinder)
  local body = cylinder(base_radius, c.body_length)
    :rotate(0, 90, 0)  -- Align along X axis
    :scale(1, c.body_width / 2, c.body_depth / 2)
    :color(0.94, 0.82, 0.76, 0.6)  -- Skin tone, semi-transparent
    :name("body_torso")
  
  return body
end

--- Create simplified head (sphere/cylinder)
function Human.create_head()
  local c = Human.config
  
  -- Head as a scaled sphere
  local head = sphere(c.head_radius)
    :at(c.body_length / 2, 0, c.head_offset_z)
    :color(0.94, 0.82, 0.76, 0.6)
    :name("body_head")
  
  return head
end

--- Create complete human cross-section
function Human.create()
  local c = Human.config
  
  local human = group("human", {
    Human.create_body(),
    Human.create_head(),
  })
  
  return human
end

--- Position human in center of bath
-- @param chamber_center_x X center of bath
-- @param chamber_center_y Y center of bath  
-- @param water_surface_z Z of water surface
function Human.position_in_bath(chamber_center_x, chamber_center_y, water_surface_z)
  local c = Human.config
  
  -- Human floats with body center slightly below water surface
  local human_z = water_surface_z - c.body_depth / 4
  
  local human = Human.create()
    :at(chamber_center_x - c.body_length / 2, chamber_center_y, human_z)
  
  return human
end

return Human
