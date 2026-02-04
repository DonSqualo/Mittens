-- Minimal test for external thread mesh generation
local Mittens = require("stdlib")
local Threads = Mittens.threads

-- Test M27 external thread directly
local ext = Threads.external({
  major_diameter = 27,
  pitch = 3,
  height = 5,
  segments_per_turn = 32
})
:name("M27 Test Thread")
:color(0.8, 0.8, 0.8, 1.0)

Mittens.register(ext)

-- Configure view to see the thread clearly
view({
  camera = {
    position = {35, 35, 15},
    target = {0, 0, 2.5},
    up = {0, 0, 1}
  }
})

return Mittens.serialize()
