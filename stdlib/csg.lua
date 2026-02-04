-- Mittens Standard Library: CSG (Constructive Solid Geometry)
-- Boolean operations on shapes

local CSG = {}

local function flatten_shapes(args, result)
  result = result or {}
  for _, arg in ipairs(args) do
    if type(arg) == "table" and arg._type == nil then
      flatten_shapes(arg, result)
    else
      table.insert(result, arg)
    end
  end
  return result
end

-- SDF factory for union operation
local function make_union_sdf(shapes)
  return function(x, y, z)
    local min_d = math.huge
    for _, shape in ipairs(shapes) do
      local d = shape:eval(x, y, z)
      if d < min_d then min_d = d end
    end
    return min_d
  end
end

-- SDF factory for difference operation
local function make_difference_sdf(base, cutters)
  return function(x, y, z)
    local d = base:eval(x, y, z)
    for _, cutter in ipairs(cutters) do
      local cut_d = -cutter:eval(x, y, z)
      if cut_d > d then d = cut_d end
    end
    return d
  end
end

-- SDF factory for intersect operation
local function make_intersect_sdf(shapes)
  return function(x, y, z)
    local max_d = -math.huge
    for _, shape in ipairs(shapes) do
      local d = shape:eval(x, y, z)
      if d > max_d then max_d = d end
    end
    return max_d
  end
end

-- Shared metatable methods for CSG results
local function make_csg_metatable()
  return {__index = {
    at = function(self, x, y, z)
      table.insert(self._ops, {op = "translate", x = x, y = y, z = z})
      return self
    end,
    rotate = function(self, rx, ry, rz)
      table.insert(self._ops, {op = "rotate", x = rx, y = ry, z = rz})
      return self
    end,
    scale = function(self, sx, sy, sz)
      sy = sy or sx
      sz = sz or sx
      table.insert(self._ops, {op = "scale", x = sx, y = sy, z = sz})
      return self
    end,
    center = function(self, axes)
      if type(axes) ~= "string" then
        error("center() expects a string like 'XY' or 'XYZ'")
      end
      local axes_upper = axes:upper()
      local cx = axes_upper:find("X") ~= nil
      local cy = axes_upper:find("Y") ~= nil
      local cz = axes_upper:find("Z") ~= nil
      local bounds = self._bounds
      local dx = cx and -((bounds.min[1] + bounds.max[1]) / 2) or 0
      local dy = cy and -((bounds.min[2] + bounds.max[2]) / 2) or 0
      local dz = cz and -((bounds.min[3] + bounds.max[3]) / 2) or 0
      table.insert(self._ops, {op = "translate", x = dx, y = dy, z = dz})
      return self
    end,
    material = function(self, mat)
      self._material = mat
      return self
    end,
    color = function(self, r, g, b, a)
      self._color = {r, g, b, a or 1.0}
      return self
    end,
    name = function(self, n)
      self._name = n
      return self
    end,
    eval = function(self, x, y, z)
      return self._sdf(x, y, z)
    end,
    serialize = function(self)
      local children_serialized = {}
      for i, child in ipairs(self._children) do
        children_serialized[i] = child:serialize()
      end
      return {
        type = "csg",
        operation = self._operation,
        children = children_serialized,
        ops = self._ops,
        material = self._material,
        color = self._color,
        name = self._name
      }
    end
  }}
end

--- Union of multiple shapes (additive)
-- @param ... Shapes to combine (can mix shapes and arrays of shapes)
-- @return Combined shape
function CSG.union(...)
  local shapes = flatten_shapes({...})

  if #shapes == 0 then
    error("union requires at least one shape")
  end

  if #shapes == 1 then
    return shapes[1]
  end

  -- Combined bounds
  local min_bounds = {math.huge, math.huge, math.huge}
  local max_bounds = {-math.huge, -math.huge, -math.huge}

  for _, shape in ipairs(shapes) do
    for i = 1, 3 do
      min_bounds[i] = math.min(min_bounds[i], shape._bounds.min[i])
      max_bounds[i] = math.max(max_bounds[i], shape._bounds.max[i])
    end
  end

  local result = {
    _type = "csg",
    _operation = "union",
    _children = shapes,
    _sdf = make_union_sdf(shapes),
    _bounds = {min = min_bounds, max = max_bounds},
    _ops = {},
    _material = nil,
  }

  setmetatable(result, make_csg_metatable())
  return result
end

--- Difference of shapes (subtractive)
-- First shape minus all subsequent shapes
-- @param base Base shape
-- @param ... Shapes to subtract (can mix shapes and arrays of shapes)
-- @return Resulting shape
function CSG.difference(base, ...)
  local cutters = flatten_shapes({...})

  if #cutters == 0 then
    return base
  end

  local children = { base }
  for _, c in ipairs(cutters) do table.insert(children, c) end

  local result = {
    _type = "csg",
    _operation = "difference",
    _children = children,
    _sdf = make_difference_sdf(base, cutters),
    _bounds = base._bounds,
    _ops = {},
    _material = nil,
  }

  setmetatable(result, make_csg_metatable())
  return result
end

-- ===========================

--- Intersection of multiple shapes (only overlapping volume)
-- @param ... Shapes to intersect (can mix shapes and arrays of shapes)
-- @return Shape containing only the volume common to all inputs
function CSG.intersect(...)
  local shapes = flatten_shapes({...})

  if #shapes == 0 then
    error("intersect requires at least one shape")
  end

  if #shapes == 1 then
    return shapes[1]
  end

  -- Intersection bounds: the overlap region of all shapes
  local min_bounds = {-math.huge, -math.huge, -math.huge}
  local max_bounds = {math.huge, math.huge, math.huge}

  for _, shape in ipairs(shapes) do
    for i = 1, 3 do
      min_bounds[i] = math.max(min_bounds[i], shape._bounds.min[i])
      max_bounds[i] = math.min(max_bounds[i], shape._bounds.max[i])
    end
  end

  -- Clamp to valid bounds (intersection may be empty)
  for i = 1, 3 do
    if min_bounds[i] > max_bounds[i] then
      min_bounds[i] = 0
      max_bounds[i] = 0
    end
  end

  local result = {
    _type = "csg",
    _operation = "intersect",
    _children = shapes,
    _sdf = make_intersect_sdf(shapes),
    _bounds = {min = min_bounds, max = max_bounds},
    _ops = {},
    _material = nil,
  }

  setmetatable(result, make_csg_metatable())
  return result
end

return CSG
