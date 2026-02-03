-- Mittens 2D Shapes and Extrusion
-- Provides polygon() and linear_extrude() for accurate profile geometry

local Shapes2D = {}

-- Shape methods (same as primitives.lua)
local shape_methods = {
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
  
  tag = function(self, t)
    self._tag = t
    return self
  end,
  
  center = function(self, cx, cy, cz)
    -- For extrusions, centering in XY
    local hw = self._metadata.width and self._metadata.width / 2 or 0
    local hd = self._metadata.depth and self._metadata.depth / 2 or 0
    local dx = cx and -hw or 0
    local dy = cy and -hd or 0
    local dz = cz and -self._metadata.height / 2 or 0
    if dx ~= 0 or dy ~= 0 or dz ~= 0 then
      table.insert(self._ops, {op = "translate", x = dx, y = dy, z = dz})
    end
    return self
  end,
  
  centered = function(self)
    return self:center(true, true, true)
  end,
  
  serialize = function(self)
    return {
      type = self._metadata.primitive,
      params = {
        points = self._metadata.points,
        holes = self._metadata.holes,
        height = self._metadata.height,
      },
      ops = self._ops,
      material = self._material,
      color = self._color,
      name = self._name,
      tag = self._tag,
    }
  end,
}

--- Create a linear extrusion from a 2D polygon
-- @param config Table with:
--   points: Array of {x, y} points for outer boundary (CCW winding)
--   holes: Optional array of arrays of {x, y} points for holes (CW winding)
--   height: Extrusion height along Z axis
-- @return Shape object that can be positioned, colored, etc.
function Shapes2D.linear_extrude(config)
  if not config.points or #config.points < 3 then
    error("linear_extrude requires at least 3 points")
  end
  
  local shape = {
    _type = "shape",
    _metadata = {
      primitive = "linear_extrude",
      points = config.points,
      holes = config.holes or {},
      height = config.height or 10,
    },
    _color = nil,
    _ops = {},
    _tag = nil,
    _name = nil,
    _material = nil,
  }
  
  setmetatable(shape, {__index = shape_methods})
  return shape
end

--- Create a 2D polygon (for use with linear_extrude)
-- @param points Array of {x, y} or {x=, y=} points
-- @return Table of normalized points
function Shapes2D.polygon(points)
  local normalized = {}
  for i, pt in ipairs(points) do
    local x = pt[1] or pt.x or 0
    local y = pt[2] or pt.y or 0
    normalized[i] = {x, y}
  end
  return normalized
end

--- Create a circular polygon approximation
-- @param radius Circle radius
-- @param segments Number of segments (default 32)
-- @param center Optional center point {x, y}
-- @return Array of points
function Shapes2D.circle(radius, segments, center)
  segments = segments or 32
  center = center or {0, 0}
  local points = {}
  for i = 0, segments - 1 do
    local angle = (i / segments) * 2 * math.pi
    points[i + 1] = {
      center[1] + radius * math.cos(angle),
      center[2] + radius * math.sin(angle)
    }
  end
  return points
end

--- Create a circular hole (CW winding)
-- @param radius Circle radius
-- @param segments Number of segments (default 32)
-- @param center Optional center point {x, y}
-- @return Array of points in CW order
function Shapes2D.circle_hole(radius, segments, center)
  segments = segments or 32
  center = center or {0, 0}
  local points = {}
  -- Reverse order for CW winding (holes)
  for i = segments - 1, 0, -1 do
    local angle = (i / segments) * 2 * math.pi
    points[#points + 1] = {
      center[1] + radius * math.cos(angle),
      center[2] + radius * math.sin(angle)
    }
  end
  return points
end

--- Create a rectangular polygon
-- @param width Rectangle width
-- @param height Rectangle height  
-- @param center If true, center at origin
-- @return Array of points (CCW)
function Shapes2D.rectangle(width, height, center)
  if center then
    local hw, hh = width/2, height/2
    return {
      {-hw, -hh},
      {hw, -hh},
      {hw, hh},
      {-hw, hh}
    }
  else
    return {
      {0, 0},
      {width, 0},
      {width, height},
      {0, height}
    }
  end
end

-- Make linear_extrude available globally
_G.linear_extrude = function(config)
  return Shapes2D.linear_extrude(config)
end

return Shapes2D
