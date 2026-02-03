-- test_simple.lua
-- Minimal test scene to verify rendering

local Mittens = require("stdlib")

-- Simple test: 3 boxes at different positions with different colors

local box1 = box(100, 100, 100)
  :at(0, 0, 0)
  :color(1.0, 0.0, 0.0, 1.0)  -- Red
  :tag("box_red")

local box2 = box(100, 100, 100)
  :at(150, 0, 0)
  :color(0.0, 1.0, 0.0, 1.0)  -- Green
  :tag("box_green")

local box3 = box(100, 100, 100)
  :at(0, 150, 0)
  :color(0.0, 0.0, 1.0, 1.0)  -- Blue
  :tag("box_blue")

local cyl = cylinder(50, 200)
  :at(200, 200, 0)
  :color(1.0, 1.0, 0.0, 1.0)  -- Yellow
  :tag("cylinder_yellow")

local assembly = group("test_assembly", {
  box1,
  box2,
  box3,
  cyl,
})

Mittens.register(assembly)

-- Close camera view
view({
  flat_shading = true,
  camera = {
    position = { 300, -400, 250 },
    target = { 100, 100, 50 },
    up = { 0, 0, 1 },
  },
  projection = "perspective",
  fov = 45,
})

print("=== Test Scene ===")
print("Expected: 3 boxes + 1 cylinder")
print("Expected colors: red, green, blue, yellow")

return Mittens.serialize()
