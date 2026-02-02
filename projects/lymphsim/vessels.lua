-- LymphSim: Vessel Networks
-- Three circulatory systems: vascular (red), glymphatic (blue), lymphatic (green)
-- Note: Avoiding small spheres due to Manifold numerical issues

local Vessels = {}

-- Vessel network configuration
Vessels.config = {
  -- Vessel radii (millimeters)
  artery_radius = 8,
  vein_radius = 6,
  capillary_radius = 2,
  glymphatic_radius = 3,
  lymph_vessel_radius = 4,
  lymph_node_radius = 12,
  
  -- Colors (RGBA)
  arterial_color = {0.9, 0.2, 0.2, 0.8},    -- Bright red
  venous_color = {0.4, 0.2, 0.6, 0.8},       -- Dark purple-red
  glymphatic_color = {0.2, 0.5, 0.9, 0.7},   -- Blue (CSF)
  lymphatic_color = {0.2, 0.8, 0.3, 0.8},    -- Green
}

--- Create vascular network (arteries and veins)
function Vessels.create_vascular()
  local c = Vessels.config
  local segments = {}
  
  -- Aorta (main artery) - runs down center
  table.insert(segments, cylinder(c.artery_radius, 250)
    :rotate(0, 90, 0)  -- Align along X
    :at(-120, 20, 0)
    :color(c.arterial_color[1], c.arterial_color[2], c.arterial_color[3], c.arterial_color[4])
    :name("aorta"))
  
  -- Vena cava (main vein) - parallel to aorta
  table.insert(segments, cylinder(c.vein_radius, 250)
    :rotate(0, 90, 0)
    :at(-120, -20, 0)
    :color(c.venous_color[1], c.venous_color[2], c.venous_color[3], c.venous_color[4])
    :name("vena_cava"))
  
  -- Branching vessels (capillary bed simulation)
  for i = 1, 5 do
    local x_pos = -100 + (i - 1) * 50
    
    -- Arterial branches (going outward)
    table.insert(segments, cylinder(c.capillary_radius, 60)
      :at(x_pos, 20, 0)
      :rotate(0, 0, 40)
      :color(c.arterial_color[1], c.arterial_color[2], c.arterial_color[3], c.arterial_color[4])
      :name("arterial_branch_" .. i))
    
    -- Venous return branches
    table.insert(segments, cylinder(c.capillary_radius, 60)
      :at(x_pos, -20, 0)
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
  local head_z = 120
  
  -- Central drainage channel
  table.insert(segments, cylinder(c.glymphatic_radius, 100)
    :at(0, 0, head_z)
    :rotate(0, 90, 0)
    :color(c.glymphatic_color[1], c.glymphatic_color[2], c.glymphatic_color[3], c.glymphatic_color[4])
    :name("glymphatic_central"))
  
  -- Perivascular spaces
  for i = 1, 3 do
    local angle = (i - 1) * 120
    local rad = math.rad(angle)
    local y_off = 30 * math.cos(rad)
    local z_off = 30 * math.sin(rad)
    
    table.insert(segments, cylinder(c.glymphatic_radius * 0.7, 60)
      :at(-20, y_off, head_z + z_off)
      :rotate(0, 90, 0)
      :color(c.glymphatic_color[1], c.glymphatic_color[2], c.glymphatic_color[3], c.glymphatic_color[4])
      :name("perivascular_" .. i))
  end
  
  -- Draining channels to cervical lymphatics
  table.insert(segments, cylinder(c.glymphatic_radius, 80)
    :at(0, 20, head_z - 40)
    :rotate(20, 0, 0)
    :color(c.glymphatic_color[1], c.glymphatic_color[2], c.glymphatic_color[3], c.glymphatic_color[4])
    :name("glymphatic_drain_L"))
  
  table.insert(segments, cylinder(c.glymphatic_radius, 80)
    :at(0, -20, head_z - 40)
    :rotate(-20, 0, 0)
    :color(c.glymphatic_color[1], c.glymphatic_color[2], c.glymphatic_color[3], c.glymphatic_color[4])
    :name("glymphatic_drain_R"))
  
  return group("glymphatic", segments)
end

--- Create lymphatic network (peripheral drainage)
function Vessels.create_lymphatic()
  local c = Vessels.config
  local segments = {}
  
  -- Main lymphatic ducts (thoracic duct)
  table.insert(segments, cylinder(c.lymph_vessel_radius, 200)
    :rotate(0, 90, 0)
    :at(-80, 50, -20)
    :color(c.lymphatic_color[1], c.lymphatic_color[2], c.lymphatic_color[3], c.lymphatic_color[4])
    :name("thoracic_duct"))
  
  -- Right lymphatic duct
  table.insert(segments, cylinder(c.lymph_vessel_radius * 0.8, 150)
    :rotate(0, 90, 0)
    :at(-60, -50, -20)
    :color(c.lymphatic_color[1], c.lymphatic_color[2], c.lymphatic_color[3], c.lymphatic_color[4])
    :name("right_lymphatic_duct"))
  
  -- Lymph nodes (use short cylinders instead of spheres)
  local node_positions = {
    {0, 80, 60},       -- Cervical (neck)
    {-50, 100, -10},   -- Axillary (armpit)
    {-50, -100, -10},  -- Axillary right
    {20, 60, -60},     -- Inguinal (groin)
    {20, -60, -60},    -- Inguinal right
  }
  
  for i, pos in ipairs(node_positions) do
    -- Use a short fat cylinder as a node (approximates sphere)
    table.insert(segments, cylinder(c.lymph_node_radius, c.lymph_node_radius * 1.5)
      :at(pos[1], pos[2], pos[3] - c.lymph_node_radius * 0.75)
      :color(c.lymphatic_color[1], c.lymphatic_color[2] * 0.8, c.lymphatic_color[3] * 0.8, c.lymphatic_color[4])
      :name("lymph_node_" .. i))
  end
  
  -- Collecting vessels (peripheral to nodes)
  for i = 1, 4 do
    local x_pos = -80 + (i - 1) * 40
    
    -- Upper body collectors
    table.insert(segments, cylinder(c.lymph_vessel_radius * 0.6, 50)
      :at(x_pos, 70, 20)
      :rotate(30, 20, 0)
      :color(c.lymphatic_color[1], c.lymphatic_color[2], c.lymphatic_color[3], c.lymphatic_color[4])
      :name("lymph_collector_upper_" .. i))
    
    -- Lower body collectors
    table.insert(segments, cylinder(c.lymph_vessel_radius * 0.6, 40)
      :at(x_pos, 50, -40)
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
function Vessels.position_in_human(human_center_x, human_center_y, human_center_z)
  local vessels = Vessels.create()
    :at(human_center_x, human_center_y, human_center_z)
  
  return vessels
end

return Vessels
