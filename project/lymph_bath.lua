-- lymph_bath.lua
-- Lymphatic drainage test bath with acoustic forcing
-- Reference: Hauglund et al. 2025 — 0.02 Hz vasomotion drives glymphatic clearance
--
-- Display: XZ plane (Y=0 slice), animated

local Mittens = require("stdlib")

-- ============================================================================
-- Configuration
-- ============================================================================

Bath = {
  length = 2000,    -- mm (X)
  width = 600,      -- mm (Y) — not rendered in 2D
  depth = 400,      -- mm (Z)
  wall = 5,         -- mm (aluminum wall thickness)
}

Gel = {
  length = 1600,    -- mm
  width = 400,      -- mm
  height = 300,     -- mm
  offset_z = 50,    -- mm from bath bottom
}

-- Simplified channel network (2D, in XZ plane)
Channels = {
  diameter = 3,     -- mm (main trunk)
  wall = 0.5,       -- mm
  
  -- Main horizontal trunk
  trunk = {
    z = Gel.offset_z + Gel.height * 0.5,
    length = Gel.length * 0.9,
  },
  
  -- Secondary channels (vertical collectors)
  collectors = {
    count = 5,
    diameter = 1,
    spacing = Gel.length / 6,
    height = Gel.height * 0.3,
  },
}

-- Acoustic sources (speakers at bath ends)
Speakers = {
  diameter = 100,
  depth = 20,
  z = Bath.depth / 2,  -- centered vertically
}

-- Simulation parameters (for physics layer)
Simulation = {
  frequency = 0.02,       -- Hz (match natural vasomotion)
  amplitude = 100,        -- Pa
  phase_sweep = 0.1,      -- Hz (moving standing wave rate)
  medium_speed = 1524,    -- m/s (water at 37°C)
}

-- ============================================================================
-- Materials
-- ============================================================================

local aluminum = material("aluminum", {
  density = 2700,
  youngs_modulus = 70e9,
  poissons_ratio = 0.33,
})

local water = material("water_37C", {
  density = 993,
  speed_of_sound = 1524,
  viscosity = 0.00069,
})

local gel = material("tissue_gel", {
  density = 1040,
  speed_of_sound = 1540,
  storage_modulus = 2000,     -- Pa (G')
  loss_modulus = 200,         -- Pa (G'')
  attenuation = 0.5,          -- dB/cm/MHz
})

local lymph = material("lymph", {
  density = 1020,
  viscosity = 0.0018,
})

-- ============================================================================
-- Geometry: Bath (outer shell)
-- ============================================================================

local bath_outer = box(
  Bath.length + 2 * Bath.wall,
  Bath.width + 2 * Bath.wall,
  Bath.depth + Bath.wall
):center(true, true, false)

local bath_inner = box(Bath.length, Bath.width, Bath.depth + 1)
  :center(true, true, false)
  :at(0, 0, Bath.wall)

local bath_shell = difference(bath_outer, bath_inner)
  :material(aluminum)
  :color(0.7, 0.7, 0.75, 1.0)
  :tag("bath_shell")

-- Water volume (for visualization)
local water_volume = box(Bath.length, Bath.width, Bath.depth)
  :center(true, true, false)
  :at(0, 0, Bath.wall)
  :material(water)
  :color(0.2, 0.4, 0.8, 0.3)
  :tag("bath_water")

-- ============================================================================
-- Geometry: Gel Block (tissue surrogate)
-- ============================================================================

local gel_block = box(Gel.length, Gel.width, Gel.height)
  :center(true, true, false)
  :at(0, 0, Bath.wall + Gel.offset_z)
  :material(gel)
  :color(0.9, 0.7, 0.7, 0.6)
  :tag("gel_matrix")

-- ============================================================================
-- Geometry: Channel Network (simplified lymphatic)
-- ============================================================================

local channel_group = {}

-- Main trunk (horizontal, in XZ plane at Y=0)
local trunk_x_start = -Channels.trunk.length / 2
local trunk_x_end = Channels.trunk.length / 2
local trunk_z = Bath.wall + Channels.trunk.z

local trunk = cylinder(Channels.diameter / 2, Channels.trunk.length)
  :centered()
  :rotate(0, 90, 0)
  :at(0, 0, trunk_z)
  :material(lymph)
  :color(0.3, 0.8, 0.3, 0.8)
  :tag("channel_trunk")

table.insert(channel_group, trunk)

-- Collector channels (vertical, connecting to trunk)
local collector_start_x = trunk_x_start + Channels.collectors.spacing
for i = 1, Channels.collectors.count do
  local x = collector_start_x + (i - 1) * Channels.collectors.spacing
  local collector = cylinder(Channels.collectors.diameter / 2, Channels.collectors.height)
    :centered()
    :at(x, 0, trunk_z + Channels.collectors.height / 2)
    :material(lymph)
    :color(0.3, 0.8, 0.3, 0.8)
    :tag("channel_collector_" .. i)
  table.insert(channel_group, collector)
end

local channels = group("lymphatic_network", channel_group)

-- ============================================================================
-- Geometry: Speakers (acoustic sources)
-- ============================================================================

local speaker_left = cylinder(Speakers.diameter / 2, Speakers.depth)
  :centered()
  :rotate(0, 90, 0)
  :at(-Bath.length / 2 - Speakers.depth / 2, 0, Bath.wall + Speakers.z)
  :color(0.2, 0.2, 0.2, 1.0)
  :tag("speaker_left")

local speaker_right = cylinder(Speakers.diameter / 2, Speakers.depth)
  :centered()
  :rotate(0, 90, 0)
  :at(Bath.length / 2 + Speakers.depth / 2, 0, Bath.wall + Speakers.z)
  :color(0.2, 0.2, 0.2, 1.0)
  :tag("speaker_right")

-- ============================================================================
-- Assembly
-- ============================================================================

local assembly = group("lymph_bath", {
  bath_shell,
  water_volume,
  gel_block,
  channels,
  speaker_left,
  speaker_right,
})

Mittens.register(assembly)

-- ============================================================================
-- Simulation Setup (consumed by physics layer)
-- ============================================================================

simulation({
  type = "acoustic_fluid",
  
  -- Acoustic field
  acoustic = {
    sources = {
      { tag = "speaker_left",  frequency = Simulation.frequency, amplitude = Simulation.amplitude, phase = 0 },
      { tag = "speaker_right", frequency = Simulation.frequency, amplitude = Simulation.amplitude, phase = math.pi },
    },
    medium = "bath_water",
    phase_sweep_rate = Simulation.phase_sweep,
  },
  
  -- Fluid network
  fluid = {
    network_tag = "lymphatic_network",
    boundary_conditions = {
      { tag = "channel_collector_1", type = "pressure", value = 0 },
      { tag = "channel_collector_5", type = "pressure", value = 0 },
    },
  },
  
  -- Time stepping
  time = {
    dt = 0.1,            -- seconds
    duration = 100,      -- seconds (2 full cycles at 0.02 Hz)
  },
  
  -- Output
  output = {
    fields = { "pressure", "velocity", "flow_rate" },
    plane = "XZ",
    plane_offset = 0,    -- Y = 0
  },
})

-- ============================================================================
-- View Configuration
-- ============================================================================

view({
  flat_shading = true,
  camera = {
    position = { 0, -2000, Bath.depth / 2 + Bath.wall },
    target = { 0, 0, Bath.depth / 2 + Bath.wall },
    up = { 0, 0, 1 },
  },
  -- XZ plane display
  projection = "orthographic",
  plane = "XZ",
})

-- ============================================================================
-- Debug Output
-- ============================================================================

print("=== Lymph Bath Configuration ===")
print(string.format("Bath: %d x %d x %d mm", Bath.length, Bath.width, Bath.depth))
print(string.format("Gel block: %d x %d x %d mm", Gel.length, Gel.width, Gel.height))
print(string.format("Main channel: d=%.1f mm, length=%.0f mm", Channels.diameter, Channels.trunk.length))
print(string.format("Collectors: %d channels, d=%.1f mm", Channels.collectors.count, Channels.collectors.diameter))
print("")
print("=== Simulation Parameters ===")
print(string.format("Frequency: %.3f Hz (period=%.1f s)", Simulation.frequency, 1/Simulation.frequency))
print(string.format("Amplitude: %.0f Pa", Simulation.amplitude))
print(string.format("Phase sweep: %.2f Hz", Simulation.phase_sweep))
print(string.format("Wavelength in water: %.1f mm", Simulation.medium_speed / Simulation.frequency * 1000))

return Mittens.serialize()
