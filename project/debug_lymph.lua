-- Debug script to inspect lymph_bath scene serialization
local Mittens = require("stdlib")

-- Load and run the lymph_bath scene
dofile("project/lymph_bath.lua")

-- Get the serialized scene
local scene = Mittens.serialize()

-- Count and inspect objects
print("\n=== Scene Debug ===")
print("Number of objects: " .. #scene.objects)

local function count_primitives(obj, depth)
  depth = depth or 0
  local indent = string.rep("  ", depth)
  local count = 0
  
  if obj.type == "group" or obj.type == "assembly" or obj.type == "component" then
    print(indent .. obj.type .. ": " .. (obj.name or "unnamed") .. " (" .. #obj.children .. " children)")
    for i, child in ipairs(obj.children) do
      count = count + count_primitives(child, depth + 1)
    end
  elseif obj.type == "csg" then
    print(indent .. "csg:" .. (obj.operation or "unknown") .. " (" .. #obj.children .. " children)")
    for i, child in ipairs(obj.children) do
      count = count + count_primitives(child, depth + 1)
    end
  else
    print(indent .. (obj.type or "unknown") .. ": " .. (obj.tag or obj.name or "unnamed"))
    count = 1
  end
  
  return count
end

for i, obj in ipairs(scene.objects) do
  print("\n--- Object " .. i .. " ---")
  local primitives = count_primitives(obj, 0)
  print("Total primitives: " .. primitives)
end

return scene
