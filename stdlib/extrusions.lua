-- Mittens Standard Library: Aluminum Extrusions
-- 80/20 style T-slot aluminum extrusion profiles
-- CSG-based implementation using box primitives and difference operations

local Extrusions = {}

local PROFILES = {
  ["20x20"] = {
    width = 20,
    depth = 20,
    wall_thickness = 1.5,
    t_slot_width = 6,
    t_slot_depth = 2.5,
  },
  
  ["40x40"] = {
    width = 40,
    depth = 40,
    wall_thickness = 2.0,
    t_slot_width = 10,
    t_slot_depth = 4,
  },
  
  ["20x40"] = {
    width = 20,
    depth = 40,
    wall_thickness = 1.5,
    t_slot_width = 6,
    t_slot_depth = 2.5,
  },
}

--- Create a T-slot extrusion profile using CSG difference
-- Builds a solid profile and subtracts T-slots from each face
-- @param profile_type "20x20", "40x40", or "20x40"
-- @param length Length of the extrusion in mm
-- @return CSG shape with T-slot profile
function Extrusions.profile(profile_type, length)
  length = length or 100
  local spec = PROFILES[profile_type]
  
  if not spec then
    error("Unknown extrusion profile: " .. tostring(profile_type))
  end

  local w = spec.width
  local d = spec.depth
  local slot_w = spec.t_slot_width
  local slot_d = spec.t_slot_depth
  
  local body = box(w, d, length)
  
  local cutouts = {}
  
  -- T-slot geometry: narrow opening + wider inner channel
  -- Opening is slot_w wide, inner channel is wider
  local inner_w = slot_w + 2  -- wider inner part of T
  local inner_d = slot_d - 1  -- depth of inner part
  
  -- Bottom face (Y=0): slot cuts into +Y direction
  local slot_bottom_outer = box(slot_w, slot_d + 0.1, length + 0.2):at(w/2 - slot_w/2, -0.1, -0.1)
  local slot_bottom_inner = box(inner_w, inner_d, length + 0.2):at(w/2 - inner_w/2, slot_d - inner_d, -0.1)
  table.insert(cutouts, slot_bottom_outer)
  table.insert(cutouts, slot_bottom_inner)
  
  -- Top face (Y=d): slot cuts into -Y direction  
  local slot_top_outer = box(slot_w, slot_d + 0.1, length + 0.2):at(w/2 - slot_w/2, d - slot_d, -0.1)
  local slot_top_inner = box(inner_w, inner_d, length + 0.2):at(w/2 - inner_w/2, d - slot_d, -0.1)
  table.insert(cutouts, slot_top_outer)
  table.insert(cutouts, slot_top_inner)
  
  -- Left face (X=0): slot cuts into +X direction
  local slot_left_outer = box(slot_d + 0.1, slot_w, length + 0.2):at(-0.1, d/2 - slot_w/2, -0.1)
  local slot_left_inner = box(inner_d, inner_w, length + 0.2):at(slot_d - inner_d, d/2 - inner_w/2, -0.1)
  table.insert(cutouts, slot_left_outer)
  table.insert(cutouts, slot_left_inner)
  
  -- Right face (X=w): slot cuts into -X direction
  local slot_right_outer = box(slot_d + 0.1, slot_w, length + 0.2):at(w - slot_d, d/2 - slot_w/2, -0.1)
  local slot_right_inner = box(inner_d, inner_w, length + 0.2):at(w - slot_d, d/2 - inner_w/2, -0.1)
  table.insert(cutouts, slot_right_outer)
  table.insert(cutouts, slot_right_inner)
  
  local result = difference(body, cutouts)
  result:name("extrusion_" .. profile_type)
  
  return result
end

--- Create a corner bracket (L-shaped reinforcement)
function Extrusions.corner_bracket(size, thickness, height)
  size = size or 40
  thickness = thickness or 5
  height = height or 40
  
  local vertical = box(thickness, size, height)
  local horizontal = box(size, thickness, height)
  
  return union(vertical, horizontal)
end

return Extrusions
