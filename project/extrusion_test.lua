-- M27 Intermediate Ring Thread Test
-- Tests ISO metric thread generation with M27 specifications
-- Creates a parametric intermediate ring with:
--   - M27 external thread on outside (male, H5)
--   - M27 internal thread on inside (female, H5)
--   - Wall thickness: 2mm

local Mittens = require("stdlib")
local Threads = Mittens.threads

-- Create M27 intermediate ring (3mm coarse pitch, 5mm height)
local m27_ring = Threads.intermediate_ring({
  size = "M27",
  height = 5,      -- "H 5" from the product specification
  wall_thickness = 2,
  pitch = 3,       -- M27 coarse pitch
})

-- Name and register for rendering
m27_ring:name("M27 Intermediate Ring")
Mittens.register(m27_ring)

-- Configure view
view({
  camera = {
    position = {50, 50, 80},
    target = {0, 0, 2.5},
    up = {0, 0, 1}
  }
})

-- Return serialized scene for rendering
return Mittens.serialize()
