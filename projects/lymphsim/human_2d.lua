-- LymphSim: Human Cross-Section (2D Slice)
-- Simplified body outline for visualization
-- Note: Avoiding small spheres due to Manifold numerical issues

local Human = {}

-- Human body dimensions (cross-section, millimeters)
Human.config = {
  body_width = 350,      -- Y: shoulder width (diameter)
  body_depth = 200,      -- Z: front-to-back (diameter)
  body_length = 40,      -- X: slice thickness
  head_radius = 80,      -- Head circle radius
  head_offset_z = 120,   -- Head center above body center
}

--- Create simplified 2D body cross-section
function Human.create_body()
  local c = Human.config
  
  -- Body as a cylinder lying along X
  local body = cylinder(c.body_width / 2, c.body_length)
    :rotate(0, 90, 0)  -- Align along X axis
    :color(0.94, 0.82, 0.76, 0.6)  -- Skin tone, semi-transparent
    :name("body_torso")
  
  return body
end

--- Create simplified head (cylinder as approximation)
-- Using a short cylinder instead of sphere to avoid Manifold numerical issues
function Human.create_head()
  local c = Human.config
  
  -- Head as a short cylinder (approximates sphere in profile)
  local head = cylinder(c.head_radius, c.body_length)
    :rotate(0, 90, 0)
    :at(0, 0, c.head_offset_z)
    :color(0.94, 0.82, 0.76, 0.6)
    :name("body_head")
  
  return head
end

--- Create complete human cross-section
function Human.create()
  local human = group("human", {
    Human.create_body(),
    Human.create_head(),
  })
  
  return human
end

--- Position human in center of bath
function Human.position_in_bath(chamber_center_x, chamber_center_y, water_surface_z)
  local c = Human.config
  
  -- Human floats with body center slightly below water surface
  local human_z = water_surface_z - c.body_depth / 4
  
  local human = Human.create()
    :at(chamber_center_x - c.body_length / 2, chamber_center_y, human_z)
  
  return human
end

return Human
