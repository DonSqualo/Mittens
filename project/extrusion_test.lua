-- T-slot profile extrusion test
-- Tests polygon extrusion with holes using Manifold

local Mittens = require("stdlib")
local Shapes2D = require("stdlib.shapes2d")

-- 40x40mm T-slot profile
local size = 40
local hs = size / 2  -- half size

-- Outer boundary (CCW)
local outer = {
  {-hs, -hs},
  {hs, -hs},
  {hs, hs},
  {-hs, hs},
}

-- T-slot openings as "holes" (CW winding)
-- Slot opening: 10mm wide, 5mm deep from each face
local slot_w = 5   -- half slot width
local slot_d = 5   -- slot depth

-- Bottom slot (y = -hs)
local bottom_slot = {
  {-slot_w, -hs + slot_d},
  {-slot_w, -hs},
  {slot_w, -hs},
  {slot_w, -hs + slot_d},
}

-- Top slot (y = +hs)
local top_slot = {
  {slot_w, hs - slot_d},
  {slot_w, hs},
  {-slot_w, hs},
  {-slot_w, hs - slot_d},
}

-- Left slot (x = -hs)
local left_slot = {
  {-hs + slot_d, slot_w},
  {-hs, slot_w},
  {-hs, -slot_w},
  {-hs + slot_d, -slot_w},
}

-- Right slot (x = +hs)
local right_slot = {
  {hs - slot_d, -slot_w},
  {hs, -slot_w},
  {hs, slot_w},
  {hs - slot_d, slot_w},
}

-- Create the extrusion with holes
local profile = linear_extrude({
  points = outer,
  holes = {bottom_slot, top_slot, left_slot, right_slot},
  height = 100,
})
  :color(0.78, 0.78, 0.80, 1.0)
  :tag("t_slot_profile")

Mittens.register(profile)

view({
  flat_shading = true,
  camera = {
    position = {120, 100, 150},
    target = {0, 0, 50},
    fov = 35
  }
})

print("=== T-Slot Profile Extrusion ===")

return Mittens.serialize()
