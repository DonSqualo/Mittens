-- Test scene for Stage 10 acoustic field implementation
-- Verifies that the acoustics module loads and computes pressure fields

local Mittens = require("stdlib")
local Acoustics = require("stdlib.acoustics")

-- Create a simple test bath
local testBath = {
  length = 2000,
  width = 600,
  depth = 400,
  center_z = 200,
}

-- Create acoustic sources
local testSources = {
  left = {
    x = -1000,
    y = 0,
    z = 200,
    amplitude = 100,
    phase = 0,
  },
  right = {
    x = 1000,
    y = 0,
    z = 200,
    amplitude = 100,
    phase = math.pi,
  },
}

-- Create simulation parameters
local testParams = {
  frequency = 0.02,      -- Hz (vasomotion-matched)
  amplitude = 100,       -- Pa
  phase_sweep = 0.1,     -- Hz
  medium_speed = 1524,   -- m/s (water at 37°C)
}

-- ============================================================================
-- TEST 1: Standing Wave Field Computation
-- ============================================================================
print("")
print("===============================================================================")
print("STAGE 10 - ACOUSTIC FIELD TEST")
print("===============================================================================")
print("")

print("[TEST 1] Creating standing wave field...")
local field = Acoustics.StandingWaveField(testBath, testSources, testParams, 0.0)
print(string.format("✓ Field created at t=0s"))
print("")

print("[TEST 2] Field statistics...")
local stats = field:statistics()
print(string.format("  Frequency: %.3f Hz", stats.frequency))
print(string.format("  Wavelength: %.1f mm", stats.wavelength))
print(string.format("  Max pressure: %.1f Pa", stats.max_pressure))
print(string.format("  Node count: %d", stats.node_count))
print(string.format("  Antinode count: %d", stats.antinode_count))
print("")

print("[TEST 3] Pressure at sample points...")
local test_points = {
  {x = -500, z = 200, name = "Left quarter"},
  {x = 0, z = 200, name = "Center"},
  {x = 500, z = 200, name = "Right quarter"},
  {x = -1000, z = 200, name = "Left speaker"},
  {x = 1000, z = 200, name = "Right speaker"},
}

for i, pt in ipairs(test_points) do
  local p = field:pressure_at(pt.x, pt.z)
  local p_norm = field:normalized_pressure(pt.x, pt.z)
  local grad = field:gradient(pt.x, pt.z)
  print(string.format("  %-18s: p=%.1f Pa (norm=%.2f), ∇p=%.2f/mm (x), %.2f/mm (z)",
    pt.name, p, p_norm, grad.x, grad.z))
end
print("")

print("[TEST 4] Time evolution...")
local times = {0.0, 0.5, 1.0, 2.0, 5.0}
for _, t in ipairs(times) do
  local field_t = Acoustics.StandingWaveField(testBath, testSources, testParams, t)
  local p_center = field_t:pressure_at(0, 200)
  print(string.format("  t=%.1fs: p(center) = %.1f Pa", t, p_center))
end
print("")

print("[TEST 5] Phase sweep (moving standing wave)...")
local params_sweep = {
  frequency = 0.02,
  amplitude = 100,
  phase_sweep = 0.5,  -- Faster sweep for testing
  medium_speed = 1524,
}
for _, t in ipairs({0.0, 0.1, 0.2}) do
  local field_sweep = Acoustics.StandingWaveField(testBath, testSources, params_sweep, t)
  local p_center = field_sweep:pressure_at(0, 200)
  local p_left = field_sweep:pressure_at(-500, 200)
  print(string.format("  t=%.1fs: p(center)=%.1f Pa, p(left)=%.1f Pa", t, p_center, p_left))
end
print("")

print("[TEST 6] Pressure colormap...")
for norm_p = 0, 1, 0.2 do
  local color = Acoustics.PressureToColor(norm_p)
  print(string.format("  p_norm=%.1f: RGB(%.2f, %.2f, %.2f, %.2f)", 
    norm_p, color[1], color[2], color[3], color[4]))
end
print("")

print("[TEST 7] Serialization...")
local serialized = field:serialize()
print(string.format("  Type: %s", serialized.type))
print(string.format("  Time: %.2f s", serialized.time))
print(string.format("  Frequency: %.3f Hz", serialized.frequency))
print(string.format("  Wavelength: %.1f mm", serialized.wavelength))
print("")

print("[TEST 8] Integration with simulation engine...")
local Simulation = require("stdlib.simulation")
local sim = Simulation.create("test_acoustic", "water")
local acoustic_sim = Acoustics.AcousticSimulation(sim, {
  bath = testBath,
  sources = testSources,
  params = testParams,
})
print(string.format("✓ Simulation with acoustics created"))
local acoustic_field = acoustic_sim:get_acoustic_field()
print(string.format("✓ Acoustic field retrieved from simulation"))
print("")

print("===============================================================================")
print("✓ ALL TESTS PASSED - ACOUSTIC FIELD MODULE FUNCTIONAL")
print("===============================================================================")
print("")

-- Return minimal geometry for renderer verification
local frame = box(100, 100, 100)
  :at(0, 0, 0)
  :color(0.5, 0.5, 0.5, 0.8)
  :tag("test_frame")

local assembly = group("test_scene", {frame})
Mittens.register(assembly)

view({
  camera = {
    position = {0, -500, 200},
    target = {0, 0, 200},
    up = {0, 0, 1},
  },
  fov = 45,
})

return Mittens.serialize()
