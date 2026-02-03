-- Simple test script
local Mittens = require("stdlib")

-- Just a simple box
local box = box(50, 50, 50)

return {
  box,
  view = {
    camera = {
      position = {-200, -200, 100},
      target = {0, 0, 0},
      up = {0, 0, 1}
    }
  }
}
