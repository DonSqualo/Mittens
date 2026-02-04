-- M27 Thread Ring Export - Final Iteration 8
-- Exports the complete M27 intermediate ring to STL format

local Mittens = require("stdlib")
local Threads = Mittens.threads
local Export = Mittens.export

-- Create M27 intermediate ring with optimized parameters
-- Total height: 25mm, Male thread: 5mm, Smooth body: 15mm, Female thread: 5mm
local m27_ring = Threads.intermediate_ring({
  size = "M27",
  height = 25,  -- Total height
  pitch = 3,    -- Coarse pitch
  segments_per_turn = 32,  -- Helix resolution (optimal for M27)
})

-- Export to STL format
Export.export_stl("~/exports/m27_thread_ring.stl", m27_ring, 128)

-- Also export to 3MF format for advanced slicers
Export.export_3mf("~/exports/m27_thread_ring.3mf", m27_ring, {
  units = "millimeter",
  color = true
})

-- Register the geometry for display
Mittens.register(m27_ring)

-- Configure view
view({
  camera = {
    position = {60, 60, 40},
    target = {0, 0, 12.5},
    up = {0, 0, 1},
    fov = 45
  }
})

return Mittens.serialize()
