-- test_acoustics.lua
-- Unit tests for stdlib/acoustics.lua acoustic field computation
--
-- Tests cover:
--   - Field computation and grid generation
--   - Pressure node/antinode detection
--   - Field interpolation
--   - Color mapping
--   - Serialization

local Acoustics = require("stdlib.acoustics")
local Test = {}

-- =============================================================================
-- Helper Functions
-- =============================================================================

local function assert_near(actual, expected, tolerance, msg)
  tolerance = tolerance or 1e-6
  msg = msg or "Values not nearly equal"
  if math.abs(actual - expected) > tolerance then
    error(string.format("%s: expected %.6f, got %.6f (diff: %.6e)",
      msg, expected, actual, math.abs(actual - expected)))
  end
end

local function assert_equal(actual, expected, msg)
  msg = msg or "Values not equal"
  if actual ~= expected then
    error(string.format("%s: expected %s, got %s", msg, tostring(expected), tostring(actual)))
  end
end

local function assert_true(value, msg)
  msg = msg or "Expected true"
  if not value then error(msg) end
end

local function assert_in_range(value, min_val, max_val, msg)
  msg = msg or string.format("Value %.6f not in range [%.6f, %.6f]", value, min_val, max_val)
  if value < min_val or value > max_val then
    error(msg)
  end
end

-- =============================================================================
-- Test 1: Default Field Creation
-- =============================================================================

function Test.test_default_field_creation()
  local field = Acoustics.create_default_field(2000, 0.02, 100)
  
  assert_true(field ~= nil, "Field should be created")
  assert_equal(field._type, "acoustic_field", "Field type mismatch")
  assert_equal(field.frequency, 0.02, "Frequency mismatch")
  assert_equal(field.amplitude, 100, "Amplitude mismatch")
  assert_true(field.max_pressure > 0, "Max pressure should be positive")
  assert_equal(field.grid_x_points, 81, "X grid points mismatch")
  assert_equal(field.grid_z_points, 41, "Z grid points mismatch")
  
  print("✓ Test 1 PASSED: Default field creation")
end

-- =============================================================================
-- Test 2: Grid Resolution and Spacing
-- =============================================================================

function Test.test_grid_spacing()
  local field = Acoustics.create_default_field(2000, 0.02, 100)
  
  -- Check grid dimensions
  assert_equal(field.grid_x_points, 81, "X points should be 81")
  assert_equal(field.grid_z_points, 41, "Z points should be 41")
  
  -- Check grid spacing
  local expected_dx = 2000 / (81 - 1)  -- 2000mm / 80 = 25mm
  local expected_dz = 400 / (41 - 1)   -- 400mm / 40 = 10mm
  
  assert_near(field.dx, expected_dx, 0.01, "DX spacing mismatch")
  assert_near(field.dz, expected_dz, 0.01, "DZ spacing mismatch")
  
  print("✓ Test 2 PASSED: Grid spacing")
end

-- =============================================================================
-- Test 3: Acoustic Parameters (Frequency, Wavelength, Wavenumber)
-- =============================================================================

function Test.test_acoustic_parameters()
  local frequency = 0.02  -- Hz (vasomotion)
  local medium_speed = 1524  -- m/s
  local field = Acoustics.create_default_field(2000, frequency, 100)
  
  -- Compute expected values
  local expected_wavelength = (medium_speed / frequency) * 1000  -- mm
  local expected_omega = 2 * math.pi * frequency                 -- rad/s
  local expected_wavenumber = 2 * math.pi / expected_wavelength  -- 1/mm
  
  -- For 0.02 Hz in 1524 m/s:
  -- wavelength = 1524 / 0.02 * 1000 = 76,200 mm
  -- omega = 2π * 0.02 ≈ 0.1257 rad/s
  
  assert_near(field.wavelength, expected_wavelength, 1, "Wavelength mismatch")
  assert_near(field.omega, expected_omega, 0.001, "Omega mismatch")
  assert_near(field.wavenumber, expected_wavenumber, 1e-5, "Wavenumber mismatch")
  
  print("✓ Test 3 PASSED: Acoustic parameters")
end

-- =============================================================================
-- Test 4: Standing Wave Field Superposition
-- =============================================================================

function Test.test_standing_wave_field()
  local field = Acoustics.create_default_field(2000, 0.02, 100)
  
  -- At time t=0, the field should have symmetric pressure distribution
  -- due to standing wave superposition
  
  -- Check that pressure data exists
  assert_true(field.pressure ~= nil, "Pressure field should exist")
  assert_true(#field.pressure > 0, "Pressure field should have data")
  
  -- Check that all grid points have pressure values
  for x_idx = 1, field.grid_x_points do
    assert_true(field.pressure[x_idx] ~= nil, 
      string.format("Pressure row %d should exist", x_idx))
    assert_equal(#field.pressure[x_idx], field.grid_z_points,
      string.format("Pressure row %d should have %d points", x_idx, field.grid_z_points))
  end
  
  -- Check pressure ranges (should be positive magnitudes)
  for x_idx = 1, field.grid_x_points do
    for z_idx = 1, field.grid_z_points do
      local p = field.pressure[x_idx][z_idx]
      assert_true(p >= 0, 
        string.format("Pressure at [%d,%d] should be non-negative, got %.2f", 
          x_idx, z_idx, p))
    end
  end
  
  print("✓ Test 4 PASSED: Standing wave field superposition")
end

-- =============================================================================
-- Test 5: Node and Antinode Detection
-- =============================================================================

function Test.test_node_antinode_detection()
  local field = Acoustics.create_default_field(2000, 0.02, 100)
  
  -- A standing wave should have identifiable nodes and antinodes
  assert_true(#field.nodes >= 0, "Nodes list should exist")
  assert_true(#field.antinodes >= 0, "Antinodes list should exist")
  
  -- For a proper standing wave, we should have more antinodes than nodes
  -- (not a strict requirement, but common for these geometry)
  
  -- Each antinode should have position and pressure data
  for _, antinode in ipairs(field.antinodes) do
    assert_true(antinode.x ~= nil, "Antinode should have x position")
    assert_true(antinode.z ~= nil, "Antinode should have z position")
    assert_true(antinode.pressure ~= nil, "Antinode should have pressure")
    assert_true(antinode.pressure > 0, "Antinode pressure should be positive")
  end
  
  -- Each node should have lower pressure than antinodes
  for _, node in ipairs(field.nodes) do
    assert_true(node.x ~= nil, "Node should have x position")
    assert_true(node.z ~= nil, "Node should have z position")
    assert_true(node.pressure ~= nil, "Node should have pressure")
    assert_true(node.pressure < field.max_pressure * 0.1,
      "Node pressure should be low (< 10% of max)")
  end
  
  print("✓ Test 5 PASSED: Node and antinode detection")
end

-- =============================================================================
-- Test 6: Pressure Interpolation
-- =============================================================================

function Test.test_pressure_interpolation()
  local field = Acoustics.create_default_field(2000, 0.02, 100)
  
  -- Test interpolation at grid points (should match stored values)
  local x = field.x_min + field.dx
  local z = field.z_min + field.dz
  local p_interp = field:pressure_at(x, z)
  
  -- Should return a valid pressure value
  assert_true(p_interp >= 0, "Interpolated pressure should be non-negative")
  assert_true(p_interp <= field.max_pressure * 1.1, 
    "Interpolated pressure should not exceed max by much")
  
  -- Test normalization
  local p_norm = field:normalized_pressure(x, z)
  assert_in_range(p_norm, 0, 1, "Normalized pressure should be in [0,1]")
  
  print("✓ Test 6 PASSED: Pressure interpolation")
end

-- =============================================================================
-- Test 7: Pressure Gradient Computation
-- =============================================================================

function Test.test_pressure_gradient()
  local field = Acoustics.create_default_field(2000, 0.02, 100)
  
  -- Compute gradient at a grid point
  local x = 0
  local z = field.z_min + 50
  local grad = field:gradient(x, z)
  
  -- Gradient should have x and z components
  assert_true(grad.x ~= nil, "Gradient should have x component")
  assert_true(grad.z ~= nil, "Gradient should have z component")
  
  -- Components should be finite numbers
  assert_true(math.abs(grad.x) < 1e10, "Gradient x should be finite")
  assert_true(math.abs(grad.z) < 1e10, "Gradient z should be finite")
  
  print("✓ Test 7 PASSED: Pressure gradient computation")
end

-- =============================================================================
-- Test 8: Color Mapping
-- =============================================================================

function Test.test_pressure_to_color()
  -- Test low pressure (node) → blue
  local color_low = Acoustics.PressureToColor(0.0)
  assert_true(color_low[3] > color_low[1], "Low pressure should be more blue than red")
  
  -- Test high pressure (antinode) → red
  local color_high = Acoustics.PressureToColor(1.0)
  assert_true(color_high[1] > color_high[3], "High pressure should be more red than blue")
  
  -- Test mid pressure → balanced
  local color_mid = Acoustics.PressureToColor(0.5)
  assert_true(color_mid[1] > 0, "Mid pressure should have red component")
  assert_true(color_mid[3] > 0, "Mid pressure should have blue component")
  
  -- Check RGBA components are in valid range
  for _, color in ipairs({color_low, color_high, color_mid}) do
    for i = 1, 4 do
      assert_in_range(color[i], 0, 1, 
        string.format("Color component %d should be in [0,1]", i))
    end
  end
  
  print("✓ Test 8 PASSED: Pressure to color mapping")
end

-- =============================================================================
-- Test 9: Field Statistics
-- =============================================================================

function Test.test_field_statistics()
  local field = Acoustics.create_default_field(2000, 0.02, 100)
  
  local stats = field:statistics()
  
  assert_true(stats.max_pressure >= 0, "Max pressure should be non-negative")
  assert_true(stats.min_pressure >= 0, "Min pressure should be non-negative")
  assert_true(stats.node_count >= 0, "Node count should be non-negative")
  assert_true(stats.antinode_count >= 0, "Antinode count should be non-negative")
  assert_equal(stats.frequency, 0.02, "Frequency in stats should match input")
  assert_equal(stats.time, 0, "Initial time should be 0")
  
  print("✓ Test 9 PASSED: Field statistics")
end

-- =============================================================================
-- Test 10: Field Serialization
-- =============================================================================

function Test.test_field_serialization()
  local field = Acoustics.create_default_field(2000, 0.02, 100)
  
  local serialized = field:serialize()
  
  assert_equal(serialized.type, "acoustic_field", "Serialized type should be acoustic_field")
  assert_equal(serialized.frequency, 0.02, "Frequency should be preserved")
  assert_equal(serialized.amplitude, 100, "Amplitude should be preserved")
  assert_true(serialized.max_pressure > 0, "Max pressure should be preserved")
  
  print("✓ Test 10 PASSED: Field serialization")
end

-- =============================================================================
-- Test 11: Time-Dependent Phase Sweep
-- =============================================================================

function Test.test_time_dependent_field()
  local field_t0 = Acoustics.create_default_field(2000, 0.02, 100)
  
  -- Create field at different time with manual time specification
  local bath_config = Acoustics.DEFAULT_BATH
  local params = Acoustics.DEFAULT_PARAMS
  local field_t1 = Acoustics.StandingWaveField(bath_config, {}, params, 1.0)
  
  -- Fields at different times should exist
  assert_equal(field_t0.time, 0, "t=0 field should have time=0")
  assert_equal(field_t1.time, 1.0, "t=1.0 field should have time=1.0")
  
  -- Phase should be different
  assert_near(field_t0.sweep_phase, 0, 0.01, "t=0 should have near-zero phase sweep")
  assert_true(field_t1.sweep_phase ~= field_t0.sweep_phase,
    "Phase sweep should differ between times")
  
  print("✓ Test 11 PASSED: Time-dependent phase sweep")
end

-- =============================================================================
-- Test 12: Custom Acoustic Source Configuration
-- =============================================================================

function Test.test_custom_source_config()
  local custom_sources = {
    left = {
      x = -1000,
      y = 0,
      z = 200,
      amplitude = 150,
      phase = 0,
    },
    right = {
      x = 1000,
      y = 0,
      z = 200,
      amplitude = 150,
      phase = math.pi,
    }
  }
  
  local bath_config = Acoustics.DEFAULT_BATH
  local params = Acoustics.DEFAULT_PARAMS
  
  local field = Acoustics.StandingWaveField(bath_config, custom_sources, params, 0)
  
  -- Field should compute with custom sources
  assert_true(field.max_pressure > 0, "Field with custom sources should have pressure")
  
  print("✓ Test 12 PASSED: Custom acoustic source configuration")
end

-- =============================================================================
-- Run All Tests
-- =============================================================================

function Test.run_all()
  print("\n" .. string.rep("=", 70))
  print("Running Acoustics Module Tests")
  print(string.rep("=", 70) .. "\n")
  
  local tests = {
    "test_default_field_creation",
    "test_grid_spacing",
    "test_acoustic_parameters",
    "test_standing_wave_field",
    "test_node_antinode_detection",
    "test_pressure_interpolation",
    "test_pressure_gradient",
    "test_pressure_to_color",
    "test_field_statistics",
    "test_field_serialization",
    "test_time_dependent_field",
    "test_custom_source_config",
  }
  
  local passed = 0
  local failed = 0
  local errors = {}
  
  for _, test_name in ipairs(tests) do
    if Test[test_name] then
      local success, err = pcall(Test[test_name])
      if success then
        passed = passed + 1
      else
        failed = failed + 1
        table.insert(errors, string.format("%s: %s", test_name, tostring(err)))
      end
    else
      error(string.format("Test function %s not found", test_name))
    end
  end
  
  print("\n" .. string.rep("=", 70))
  print(string.format("Results: %d passed, %d failed", passed, failed))
  print(string.rep("=", 70))
  
  if failed > 0 then
    print("\nErrors:")
    for _, error_msg in ipairs(errors) do
      print("  • " .. error_msg)
    end
    return false
  end
  
  return true
end

-- Run if executed as main script
if arg and arg[0] and arg[0]:match("test_acoustics") then
  Test.run_all()
end

return Test
