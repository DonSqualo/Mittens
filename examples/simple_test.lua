-- Simple test script
local Mittens = require("stdlib")

-- Just a simple box
local b = box(50, 50, 50)
Mittens.register(b)

-- Set camera view
Mittens.view.view({
  camera = {
    position = {-200, -200, 100},
    target = {0, 0, 0}
  }
})

return Mittens.serialize()
