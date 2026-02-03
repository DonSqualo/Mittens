-- Test scene: T-slot extrusion frame
-- Verifies that the CSG-based extrusion implementation works correctly

local Mittens = require("stdlib")

local extrusion_length = 200

local vertical_left = extrusion("20x20", extrusion_length)
  :at(0, 0, 0)
  :color(0.85, 0.85, 0.85, 1.0)

local vertical_right = extrusion("20x20", extrusion_length)
  :at(100, 0, 0)
  :color(0.85, 0.85, 0.85, 1.0)

local horizontal_front = extrusion("20x20", 100)
  :rotate(0, 0, 90)
  :at(0, 0, 0)
  :color(0.8, 0.8, 0.8, 1.0)

local horizontal_back = extrusion("20x20", 100)
  :rotate(0, 0, 90)
  :at(0, 80, extrusion_length - 20)
  :color(0.8, 0.8, 0.8, 1.0)

local frame = group("extrusion_frame", {
  vertical_left,
  vertical_right,
  horizontal_front,
  horizontal_back
})

Mittens.register(frame)

view({
  camera = {
    position = {60, 60, 150},
    target = {50, 40, 100},
    up = {0, 0, 1}
  }
})

return Mittens.serialize()
