-- Simple test: just show a box at origin
local Mittens = require("stdlib")

-- A simple red box
local test_box = box(500, 300, 200)
  :color(0.9, 0.2, 0.2, 1.0)
  :name("test_box")

-- Set up view - use same style as working examples
view({
  camera = "isometric",
  distance = 1000,
  target = { 250, 150, 100 },
  theme = "dark",
  axes = { show = true, size = 100 },
})

Mittens.register(test_box)
return Mittens.serialize()
