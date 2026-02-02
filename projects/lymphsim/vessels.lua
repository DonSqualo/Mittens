-- LymphSim: Vessel Networks
-- Three circulatory systems: vascular (red), glymphatic (blue), lymphatic (green)

local Vessels = {}

-- Vessel network configuration
Vessels.config = {
  -- Vessel radii (meters)
  artery_radius = 0.008,
  vein_radius = 0.006,
  capillary_radius = 0.002,
  glymphatic_radius = 0.003,
  lymph_vessel_radius = 0.004,
  lymph_node_radius = 0.012,
  
  -- Colors (RGBA)
  arterial_color = {0.9, 0.2, 0.2, 0.8},    -- Bright red
  venous_color = {0.4, 0.2, 0.6, 0.8},       -- Dark purple-red
  glymphatic_color = {0.2, 0.5, 0.9, 0.7},   -- Blue (CSF)
  lymphatic_color = {0.2, 0.8, 0.3, 0.8},    -- Green
}

--- Create a vessel segment (cylinder between two points)
-- @param start_pos {x, y, z} start position
-- @param end_pos {x, y, z} end position
-- @param radius tube radius
-- @param color {r, g, b, a} color
-- @param name segment name
local function create_vessel_segment(start_pos, end_pos, radius, color, name)
  -- Calculate length and direction
  local dx = end_pos[1] - start_pos[1]
  local dy = end_pos[2] - start_pos[2]
  local dz = end_pos[3] - start_pos[3]
  local length = math.sqrt(dx*dx + dy*dy + dz*dz)
  
  if length < 0.001 then return nil end
  
  -- Calculate rotation angles to align cylinder along direction
  -- Cylinder extends along +Z by default
  local horizontal = math.sqrt(dx*dx + dy*dy)
  local pitch = math.deg(math.atan2(horizontal, dz))  -- Rotation around Y
  local yaw = math.deg(math.atan2(dx, dy))            -- Rotation around Z
  
  local segment = cylinder(radius, length)
    :at(start_pos[1], start_pos[2], start_pos[3])
    :rotate(pitch, 0, -yaw)
    :color(color[1], color[2], color[3], color[4])
    :name(name)
  
  return segment
end

--- Create vascular network (arteries and veins)
function Vessels.create_vascular()
  local c = Vessels.config
  local segments = {}
  
  -- Main vessels running along body (X direction)
  -- Centered in body at Y=0, Z=0 (relative to human center)
  
  -- Aorta (main artery) - runs down center
  table.insert(segments, cylinder(c.artery_radius, 0.25)
    :rotate(0, 90, 0)  -- Align along X
    :at(-0.12, 0.02, 0)
    :color(c.arterial_color[1], c.arterial_color[2], c.arterial_color[3], c.arterial_color[4])
    :name("aorta"))
  
  -- Vena cava (main vein) - parallel to aorta
  table.insert(segments, cylinder(c.vein_radius, 0.25)
    :rotate(0, 90, 0)
    :at(-0.12, -0.02, 0)
    :color(c.venous_color[1], c.venous_color[2], c.venous_color[3], c.venous_color[4])
    :name("vena_cava"))
  
  -- Branching vessels (capillary bed simulation)
  for i = 1, 5 do
    local x_pos = -0.10 + (i - 1) * 0.05
    
    -- Arterial branches (going outward)
    table.insert(segments, cylinder(c.capillary_radius, 0.06)
      :at(x_pos, 0.02, 0)
      :rotate(0, 0, 40)
      :color(c.arterial_color[1], c.arterial_color[2], c.arterial_color[3], c.arterial_color[4])
      :name("arterial_branch_" .. i))
    
    -- Venous return branches
    table.insert(segments, cylinder(c.capillary_radius, 0.06)
      :at(x_pos, -0.02, 0)
      :rotate(0, 0, -40)
      :color(c.venous_color[1], c.venous_color[2], c.venous_color[3], c.venous_color[4])
      :name("venous_branch_" .. i))
  end
  
  return group("vascular", segments)
end

--- Create glymphatic network (brain/CSF drainage)
function Vessels.create_glymphatic()
  local c = Vessels.config
  local segments = {}
  
  -- Glymphatic vessels primarily in head region
  -- Perivascular spaces around arteries
  
  -- Main glymphatic channels in head (at head_offset_z ~ 0.12)
  local head_z = 0.12
  
  -- Central drainage channel
  table.insert(segments, cylinder(c.glymphatic_radius, 0.10)
    :at(0, 0, head_z)
    :rotate(0, 90, 0)
    :color(c.glymphatic_color[1], c.glymphatic_color[2], c.glymphatic_color[3], c.glymphatic_color[4])
    :name("glymphatic_central"))
  
  -- Perivascular spaces (ring around vessels)
  for i = 1, 3 do
    local angle = (i - 1) * 120
    local rad = math.rad(angle)
    local y_off = 0.03 * math.cos(rad)
    local z_off = 0.03 * math.sin(rad)
    
    table.insert(segments, cylinder(c.glymphatic_radius * 0.7, 0.06)
      :at(-0.02, y_off, head_z + z_off)
      :rotate(0, 90, 0)
      :color(c.glymphatic_color[1], c.glymphatic_color[2], c.glymphatic_color[3], c.glymphatic_color[4])
      :name("perivascular_" .. i))
  end
  
  -- Draining channels to cervical lymphatics
  table.insert(segments, cylinder(c.glymphatic_radius, 0.08)
    :at(0, 0.02, head_z - 0.04)
    :rotate(20, 0, 0)
    :color(c.glymphatic_color[1], c.glymphatic_color[2], c.glymphatic_color[3], c.glymphatic_color[4])
    :name("glymphatic_drain_L"))
  
  table.insert(segments, cylinder(c.glymphatic_radius, 0.08)
    :at(0, -0.02, head_z - 0.04)
    :rotate(-20, 0, 0)
    :color(c.glymphatic_color[1], c.glymphatic_color[2], c.glymphatic_color[3], c.glymphatic_color[4])
    :name("glymphatic_drain_R"))
  
  return group("glymphatic", segments)
end

--- Create lymphatic network (peripheral drainage)
function Vessels.create_lymphatic()
  local c = Vessels.config
  local segments = {}
  
  -- Lymphatic vessels throughout body, draining toward lymph nodes
  
  -- Main lymphatic ducts (thoracic duct)
  table.insert(segments, cylinder(c.lymph_vessel_radius, 0.20)
    :rotate(0, 90, 0)
    :at(-0.08, 0.05, -0.02)
    :color(c.lymphatic_color[1], c.lymphatic_color[2], c.lymphatic_color[3], c.lymphatic_color[4])
    :name("thoracic_duct"))
  
  -- Right lymphatic duct
  table.insert(segments, cylinder(c.lymph_vessel_radius * 0.8, 0.15)
    :rotate(0, 90, 0)
    :at(-0.06, -0.05, -0.02)
    :color(c.lymphatic_color[1], c.lymphatic_color[2], c.lymphatic_color[3], c.lymphatic_color[4])
    :name("right_lymphatic_duct"))
  
  -- Lymph nodes (clusters)
  local node_positions = {
    {0, 0.08, 0.06},      -- Cervical (neck)
    {-0.05, 0.10, -0.01}, -- Axillary (armpit)
    {-0.05, -0.10, -0.01}, -- Axillary right
    {0.02, 0.06, -0.06},  -- Inguinal (groin)
    {0.02, -0.06, -0.06}, -- Inguinal right
  }
  
  for i, pos in ipairs(node_positions) do
    table.insert(segments, sphere(c.lymph_node_radius)
      :at(pos[1], pos[2], pos[3])
      :color(c.lymphatic_color[1], c.lymphatic_color[2] * 0.8, c.lymphatic_color[3] * 0.8, c.lymphatic_color[4])
      :name("lymph_node_" .. i))
  end
  
  -- Collecting vessels (peripheral to nodes)
  for i = 1, 4 do
    local x_pos = -0.08 + (i - 1) * 0.04
    
    -- Upper body collectors
    table.insert(segments, cylinder(c.lymph_vessel_radius * 0.6, 0.05)
      :at(x_pos, 0.07, 0.02)
      :rotate(30, 20, 0)
      :color(c.lymphatic_color[1], c.lymphatic_color[2], c.lymphatic_color[3], c.lymphatic_color[4])
      :name("lymph_collector_upper_" .. i))
    
    -- Lower body collectors
    table.insert(segments, cylinder(c.lymph_vessel_radius * 0.6, 0.04)
      :at(x_pos, 0.05, -0.04)
      :rotate(-30, 10, 0)
      :color(c.lymphatic_color[1], c.lymphatic_color[2], c.lymphatic_color[3], c.lymphatic_color[4])
      :name("lymph_collector_lower_" .. i))
  end
  
  return group("lymphatic", segments)
end

--- Create all vessel networks
function Vessels.create()
  return group("vessels", {
    Vessels.create_vascular(),
    Vessels.create_glymphatic(),
    Vessels.create_lymphatic(),
  })
end

--- Position vessels relative to human center
-- @param human_center_x X position of human
-- @param human_center_y Y position of human
-- @param human_center_z Z position of human
function Vessels.position_in_human(human_center_x, human_center_y, human_center_z)
  local vessels = Vessels.create()
    :at(human_center_x, human_center_y, human_center_z)
  
  return vessels
end

return Vessels
