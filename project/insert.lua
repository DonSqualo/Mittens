-- Zeiss Microscope M27 x 0.75 Thread Parts
-- Male (external) and Female (internal) threads side by side

local Mittens = require("stdlib")
local Threads = Mittens.threads

-- === MALE PART (external thread) ===
-- Class 6g: 0.2mm clearance for 3D printing (undersized from nominal)
local male_thread = Threads.external({
  major_diameter = 27,
  pitch = 0.75,
  height = 10,
  segments_per_turn = 32,
  clearance = 0.2  -- mm, for 3D printing
})

-- Make male part hollow
local male_bore = cylinder(10, 10.1):at(0, 0, -0.05)
local male_part = difference(male_thread, male_bore)
:name("M27 Male")
:color(0.75, 0.75, 0.8, 1.0)
:at(-30, 0, 0)  -- Back to side by side for now

-- === FEMALE PART (internal thread) ===
-- internal_thread primitive creates tube with threaded bore directly
local female_part = Threads.internal({
  major_diameter = 27,
  pitch = 0.75,
  height = 10,
  segments_per_turn = 32
})
:name("M27 Female")
:color(0.7, 0.75, 0.8, 1.0)
:at(30, 0, 0)  -- Back to side by side

Mittens.register(male_part)
Mittens.register(female_part)

-- Camera to see both parts
view({
  camera = {
    position = {70, 30, 20},
    target = {0, 0, 5},
    up = {0, 0, 1},
    fov = 45
  },
  flat_shading = true
})

-- Serialize and add exports
local result = Mittens.serialize()

result.exports = {
  {
    format = "stl",
    filename = "m27_male.stl",
    object = male_part:serialize(),
  },
  {
    format = "stl", 
    filename = "m27_female.stl",
    object = female_part:serialize(),
  }
}

return result
