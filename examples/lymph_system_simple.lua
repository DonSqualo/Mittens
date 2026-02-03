-- lymph_system_simple.lua
-- Simplified lymphatic system visualization

local Mittens = require("stdlib")

-- Minimal lymphatic network for testing
local lymph_material = material("lymph", {
  density = 1020,
  viscosity = 0.0018,
})

-- Create a simple tube structure
local tube = cylinder(5, 100)
  :centered()
  :material(lymph_material)
  :color(0.2, 1.0, 0.2, 1.0)
  :tag("lymph_vessel")

local assembly = group("lymph_system", { tube })
Mittens.register(assembly)

-- Set camera view
Mittens.view.view({
  camera = {
    position = {-2554, -2061, 1395},
    target = {0, 0, 20}
  }
})

return Mittens.serialize()
