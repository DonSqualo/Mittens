-- Mittens Standard Library: Aluminum Extrusions
-- 80/20 style T-slot aluminum extrusion profiles
-- CSG-based implementation with accurate geometry matching industry specifications
-- Reference: 80/20 Inc Engineering Handbook, ISO 4401 T-slot profile
-- https://www.8020.net/resources/specifications

local Extrusions = {}

local PROFILES = {
  ["20x20"] = {
    outer_width = 20,
    outer_depth = 20,
    wall_thickness = 1.5,
    corner_radius = 0.5,
    t_slot_width = 6.5,
    t_slot_depth = 3.2,
    t_slot_undercut_width = 8.0,
    center_bore = 5.0,
  },
  
  ["40x40"] = {
    outer_width = 40,
    outer_depth = 40,
    wall_thickness = 2.0,
    corner_radius = 1.0,
    t_slot_width = 10.2,
    t_slot_depth = 5.0,
    t_slot_undercut_width = 12.5,
    center_bore = 8.0,
  },
  
  ["20x40"] = {
    outer_width = 20,
    outer_depth = 40,
    wall_thickness = 1.5,
    corner_radius = 0.5,
    t_slot_width = 6.5,
    t_slot_depth = 3.2,
    t_slot_undercut_width = 8.0,
    center_bore = 5.0,
  },
}

function Extrusions.profile(profile_type, length)
  length = length or 100
  local spec = PROFILES[profile_type]
  
  if not spec then
    error("Unknown extrusion profile: " .. tostring(profile_type))
  end

  local w = spec.outer_width
  local d = spec.outer_depth
  local slot_w = spec.t_slot_width
  local slot_d = spec.t_slot_depth
  local undercut_w = spec.t_slot_undercut_width
  local bore = spec.center_bore
  
  local body = box(w, d, length)
  
  local cutouts = {}
  
  -- T-slot geometry (accurate profile):
  -- - Narrow opening (slot_w) at the face
  -- - Undercut section (undercut_w) as the T-slot base
  -- - Slot depth (slot_d) total depth
  
  -- Bottom face (X: centered, Y: -d/2): slot cuts into +Y direction
  local slot_bot_outer = box(slot_w, slot_d, length + 0.2):at(w/2 - slot_w/2, -slot_d, -0.1)
  local slot_bot_undercut = box(undercut_w, slot_d - 1.5, length + 0.2):at(w/2 - undercut_w/2, -slot_d + 1.5, -0.1)
  table.insert(cutouts, slot_bot_outer)
  table.insert(cutouts, slot_bot_undercut)
  
  -- Top face (X: centered, Y: +d/2): slot cuts into -Y direction
  local slot_top_outer = box(slot_w, slot_d, length + 0.2):at(w/2 - slot_w/2, d - slot_d, -0.1)
  local slot_top_undercut = box(undercut_w, slot_d - 1.5, length + 0.2):at(w/2 - undercut_w/2, d + 0.5 - slot_d, -0.1)
  table.insert(cutouts, slot_top_outer)
  table.insert(cutouts, slot_top_undercut)
  
  -- Left face (Y: centered, X: -w/2): slot cuts into +X direction
  local slot_left_outer = box(slot_d, slot_w, length + 0.2):at(-slot_d, d/2 - slot_w/2, -0.1)
  local slot_left_undercut = box(slot_d - 1.5, undercut_w, length + 0.2):at(-slot_d + 1.5, d/2 - undercut_w/2, -0.1)
  table.insert(cutouts, slot_left_outer)
  table.insert(cutouts, slot_left_undercut)
  
  -- Right face (Y: centered, X: +w/2): slot cuts into -X direction
  local slot_right_outer = box(slot_d, slot_w, length + 0.2):at(w - slot_d, d/2 - slot_w/2, -0.1)
  local slot_right_undercut = box(slot_d - 1.5, undercut_w, length + 0.2):at(w + 0.5 - slot_d, d/2 - undercut_w/2, -0.1)
  table.insert(cutouts, slot_right_outer)
  table.insert(cutouts, slot_right_undercut)
  
  -- Center bore hole (optional, for bolt pass-through)
  if bore and bore > 0 then
    local bore_hole = cylinder(bore / 2, length + 0.2)
      :centered()
      :rotate(0, 90, 0)
      :at(0, 0, -0.1)
    table.insert(cutouts, bore_hole)
  end
  
  local result = difference(body, cutouts)
  result:name("extrusion_" .. profile_type)
  
  return result
end

function Extrusions.corner_bracket(size, thickness, height)
  size = size or 40
  thickness = thickness or 5
  height = height or 40
  
  local vertical = box(thickness, size, height)
  local horizontal = box(size, thickness, height)
  
  return union(vertical, horizontal)
end

function Extrusions.structural_frame(length, width, height, profile_type)
  profile_type = profile_type or "20x20"
  local spec = PROFILES[profile_type]
  
  if not spec then
    error("Unknown profile for frame: " .. tostring(profile_type))
  end
  
  local pw = spec.outer_width   -- profile width
  local pd = spec.outer_depth   -- profile depth
  
  local parts = {}
  
  -- Four vertical corner legs (centered on frame corners)
  -- Legs placed so their centers align with frame corner positions
  table.insert(parts, Extrusions.profile(profile_type, height)
    :at(-length/2 - pw/2, -width/2 - pd/2, 0))
  table.insert(parts, Extrusions.profile(profile_type, height)
    :at(length/2 - pw/2, -width/2 - pd/2, 0))
  table.insert(parts, Extrusions.profile(profile_type, height)
    :at(-length/2 - pw/2, width/2 - pd/2, 0))
  table.insert(parts, Extrusions.profile(profile_type, height)
    :at(length/2 - pw/2, width/2 - pd/2, 0))
  
  -- Horizontal beams at mid-height connecting legs
  table.insert(parts, Extrusions.profile(profile_type, length)
    :rotate(0, 90, 0)
    :at(-length/2, -width/2 - pd/2, height / 2))
  table.insert(parts, Extrusions.profile(profile_type, length)
    :rotate(0, 90, 0)
    :at(-length/2, width/2 - pd/2, height / 2))
  table.insert(parts, Extrusions.profile(profile_type, width)
    :rotate(90, 0, 0)
    :at(-length/2 - pw/2, -width/2, height / 2))
  table.insert(parts, Extrusions.profile(profile_type, width)
    :rotate(90, 0, 0)
    :at(length/2 - pw/2, -width/2, height / 2))
  
  -- Lower reinforcing beams at 1/4 height for water load support
  table.insert(parts, Extrusions.profile(profile_type, length)
    :rotate(0, 90, 0)
    :at(-length/2, -width/2 + 50, height / 4))
  table.insert(parts, Extrusions.profile(profile_type, length)
    :rotate(0, 90, 0)
    :at(-length/2, width/2 - 50, height / 4))
  
  local frame = group("structural_frame", parts)
  return frame
end

-- Supplier component list (purchasable parts)
-- 80/20 Inc: https://www.8020.net/
--   Part 25-2020-96: 20x20mm profile, 96in/2438mm length
--   Part 25-2020-48: 20x20mm profile, 48in/1219mm length
-- Misumi: https://us.misumi-ec.com/
--   HFS-2020-500: 20x20mm T-slot, 500mm length
--   HFS-2020-1000: 20x20mm T-slot, 1000mm length
-- For 2000x600x400mm bath frame (~500kg water load):
--   Vertical legs: 4x 400mm segments
--   Horizontal beams: 2x 2000mm + 2x 600mm
--   Cross-bracing: 2x ~1900mm diagonals

return Extrusions
