-- Unit test for extrusion CSG implementation
-- Verifies the extrusion profile builds valid CSG geometry

local Mittens = require("stdlib")

local function test_extrusion_profile()
  print("\n=== Testing Extrusion CSG Implementation ===\n")
  
  print("Test 1: Creating 20x20 extrusion with 200mm length")
  local ext = extrusion("20x20", 200)
  print("  ✓ Extrusion created")
  print("  Type: " .. ext._type)
  print("  Operation: " .. ext._operation)
  print("  Children: " .. #ext._children)
  
  print("\nTest 2: Extrusion has CSG structure")
  assert(ext._type == "csg", "Extrusion should be a CSG operation")
  assert(ext._operation == "difference", "Should be a difference operation")
  assert(#ext._children == 5, "Should have 1 body + 4 T-slot cutouts")
  print("  ✓ CSG structure is correct")
  
  print("\nTest 3: SDF evaluation at different points")
  local center_dist = ext:eval(10, 10, 100)
  print(string.format("  Distance at center (10,10,100): %.3f", center_dist))
  
  local edge_dist = ext:eval(0, 0, 100)
  print(string.format("  Distance at edge (0,0,100): %.3f", edge_dist))
  
  local outside_dist = ext:eval(-5, -5, 100)
  print(string.format("  Distance outside (-5,-5,100): %.3f", outside_dist))
  
  print("\nTest 4: Bounds are correct")
  local bounds = ext._bounds
  print(string.format("  Min: (%.1f, %.1f, %.1f)", bounds.min[1], bounds.min[2], bounds.min[3]))
  print(string.format("  Max: (%.1f, %.1f, %.1f)", bounds.max[1], bounds.max[2], bounds.max[3]))
  assert(bounds.max[3] == 200, "Height should be 200mm")
  print("  ✓ Bounds are correct")
  
  print("\nTest 5: Extrusion with transformation")
  local ext_transformed = extrusion("20x20", 100)
    :at(50, 100, 0)
    :color(0.8, 0.8, 0.8, 1.0)
  assert(#ext_transformed._ops == 2, "Should have 2 operations (at + color metadata)")
  print("  ✓ Transformations applied")
  
  print("\nTest 6: Different profile sizes")
  local ext_40x40 = extrusion("40x40", 100)
  print("  ✓ 40x40 profile created")
  local ext_20x40 = extrusion("20x40", 100)
  print("  ✓ 20x40 profile created")
  
  print("\nTest 7: Error handling for invalid profile")
  local ok, err = pcall(function()
    extrusion("invalid", 100)
  end)
  assert(not ok, "Should error on invalid profile")
  print("  ✓ Invalid profile rejected: " .. err)
  
  print("\n=== All Tests Passed ===\n")
  return true
end

if test_extrusion_profile() then
  print("SUCCESS: Extrusion CSG implementation is working correctly!")
  os.exit(0)
else
  print("FAILURE: Extrusion CSG implementation has issues")
  os.exit(1)
end
