-- M27-M27 Intermediate Ring (Adapter)
-- Total height: 25mm
-- Male thread (bottom): 5mm  
-- Smooth body (middle): 15mm
-- Female thread (top): 5mm
-- M27 coarse pitch: 3mm

local Mittens = require("stdlib")
local Threads = Mittens.threads

-- Dimensions
local total_height = 25
local thread_height = 5
local smooth_height = total_height - (2 * thread_height)  -- 15mm
local outer_diameter = 27  -- M27
local wall_thickness = 2
local inner_diameter = outer_diameter - (2 * wall_thickness)  -- 23mm
local pitch = 3

-- Create external thread (male) at bottom
local male_thread = Threads.external({
  major_diameter = outer_diameter,
  pitch = pitch,
  height = thread_height,
}):name("M27 external thread")

-- Create internal thread (female) at top  
local female_thread = Threads.internal({
  major_diameter = inner_diameter,
  pitch = pitch,
  height = thread_height,
}):at(0, 0, total_height - thread_height)
 :name("M27 internal thread")

-- Create smooth body section in the middle
-- This is a simple ring (annulus)
local smooth_body = ring(inner_diameter/2 - 0.5, outer_diameter/2 + 0.5, smooth_height)
  :at(0, 0, thread_height)
  :name("Smooth body")

-- Combine all parts
local ring_assembly = group("M27 Intermediate Ring H25", {
  male_thread,
  smooth_body,
  female_thread,
})

Mittens.register(ring_assembly)

-- Configure view - pull back for taller part
view({
  camera = {
    position = {60, 60, 40},
    target = {0, 0, 12.5},
    up = {0, 0, 1},
    fov = 45
  }
})

return Mittens.serialize()
