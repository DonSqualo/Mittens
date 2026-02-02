-- LymphSim: Main Entry Point
-- Lymphatic drainage bath simulation - Phase 1: Static Geometry

local Mittens = require("stdlib")

-- Load sub-modules
local Chamber = require("projects.lymphsim.chamber")
local Human = require("projects.lymphsim.human_2d")
local Vessels = require("projects.lymphsim.vessels")

-- Create the bath chamber
local chamber = Chamber.create()

-- Position human in center of bath
local human = Human.position_in_bath(
  Chamber.center_x,
  Chamber.center_y,
  Chamber.water_surface_z
)

-- Get human center for vessel positioning
local human_center_x = Chamber.center_x
local human_center_y = Chamber.center_y
local human_center_z = Chamber.water_surface_z - Human.config.body_depth / 4

-- Position vessels inside human
local vessels = Vessels.position_in_human(
  human_center_x,
  human_center_y,
  human_center_z
)

-- Create the complete assembly
local lymphsim = assembly("LymphSim", {
  chamber,
  human,
  vessels,
}, {
  author = "LymphSim Phase 1",
  description = "2m lymphatic drainage bath with 3-compartment vessel visualization",
  version = "0.1.0",
})

-- Set up the view
view({
  camera = {
    position = {2.5, 1.5, 1.2},
    target = {Chamber.center_x, Chamber.center_y, Chamber.water_surface_z},
    up = {0, 0, 1},
  },
  background = {0.1, 0.1, 0.15, 1.0},
})

-- Register the assembly
Mittens.register(lymphsim)

return Mittens.serialize()
