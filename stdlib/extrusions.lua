-- Mittens Standard Library: Aluminum Extrusions
-- 80/20 style T-slot aluminum extrusion profiles
-- Reference: 80/20 Inc. standard extrusion specifications
-- https://www.80020.net/
-- 
-- Provides helper functions to build extrusion frames using native Mittens primitives

local Extrusions = {}

-- ============================================================================
-- T-Slot Profile Specifications (80/20 compatible dimensions in mm)
-- ============================================================================

local PROFILES = {
  -- 20x20mm profile
  ["20x20"] = {
    width = 20,
    depth = 20,
    wall_thickness = 1.5,
    t_slot_width = 6,
    t_slot_depth = 2.5,
    corner_radius = 2.5,
    hole_diameter = 5,  -- mounting holes
  },
  
  -- 40x40mm profile
  ["40x40"] = {
    width = 40,
    depth = 40,
    wall_thickness = 2.0,
    t_slot_width = 10,
    t_slot_depth = 4,
    corner_radius = 3.0,
    hole_diameter = 8,  -- mounting holes
  },
  
  -- 20x40mm profile (rectangular)
  ["20x40"] = {
    width = 20,
    depth = 40,
    wall_thickness = 1.5,
    t_slot_width = 6,
    t_slot_depth = 2.5,
    corner_radius = 2.5,
    hole_diameter = 5,  -- mounting holes
  },
}

-- ============================================================================
-- Extrusion Profile Generation
-- ============================================================================

--- Create a T-slot extrusion profile
-- Returns a native Mittens box primitive sized for the extrusion
-- @param profile_type "20x20", "40x40", or "20x40"
-- @param length Length of the extrusion in mm (along Z axis)
-- @return Extrusion box primitive
function Extrusions.profile(profile_type, length)
  length = length or 100
  local spec = PROFILES[profile_type]
  
  if not spec then
    error("Unknown extrusion profile: " .. tostring(profile_type))
  end
  
  -- Create and return a simple box for the extrusion body
  -- Dimensions: width x depth x length
  local extrusion = box(spec.width, spec.depth, length)
  return extrusion
end

--- Create a corner bracket (L-shaped reinforcement)
-- Returns two perpendicular extrusion pieces as a group
-- @param size Size of the bracket (40mm by default for 40x40 profiles)
-- @param thickness Thickness of the bracket material
-- @param height Height of the bracket
-- @return Group of two bracket pieces
function Extrusions.corner_bracket(size, thickness, height)
  size = size or 40
  thickness = thickness or 5
  height = height or 40
  
  -- Create two perpendicular arms
  local vertical = box(thickness, size, height)
  local horizontal = box(size, thickness, height)
  
  return group("corner_bracket", {vertical, horizontal})
end

-- ============================================================================
-- Export
-- ============================================================================

return Extrusions
