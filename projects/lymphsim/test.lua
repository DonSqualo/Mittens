-- Simple test: just show a box at origin
local Mittens = require("stdlib")

-- A simple red box
local test_box = box(500, 300, 200)
  :color(0.9, 0.2, 0.2, 1.0)
  :name("test_box")

-- Set up view - camera looking at origin
view({
  camera = {
    position = {1000, 1000, 800},
    target = {0, 0, 0},
    up = {0, 0, 1},
  },
  background = {0.1, 0.1, 0.15, 1.0},
})

Mittens.register(test_box)
return Mittens.serialize()
