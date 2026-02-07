-- Mittens Standard Library: Primitives
-- Basic geometric primitives using Signed Distance Functions (SDF)

local Primitives = {}

-- Create a shape object with SDF and metadata
local function Shape(sdf_func, bounds, metadata)
  local shape = {
    _type = "shape",
    _sdf = sdf_func,
    _bounds = bounds or {min = {-1e6, -1e6, -1e6}, max = {1e6, 1e6, 1e6}},
    _ops = {},
    _material = nil,
    _metadata = metadata or {}
  }

  setmetatable(shape, {__index = {
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

    eval = function(self, x, y, z)
      return self._sdf(x, y, z)
    end,

    serialize = function(self)
      return {
        type = self._metadata.primitive,
        params = self._metadata.params,
        ops = self._ops,
        material = self._material,
        color = self._color,
        name = self._name
      }
    end
  }})

  return shape
end

-- SDF for box (corner at origin, extends to +w, +d, +h)
local function box_sdf(w, d, h)
  return function(x, y, z)
    local qx = math.max(-x, x - w)
    local qy = math.max(-y, y - d)
    local qz = math.max(-z, z - h)
    local outside = math.sqrt(
      math.max(qx, 0)^2 +
      math.max(qy, 0)^2 +
      math.max(qz, 0)^2
    )
    local inside = math.min(math.max(qx, qy, qz), 0)
    return outside + inside
  end
end

-- SDF for cylinder (base on XY plane, extends along +Z)
local function cylinder_sdf(r, h)
  return function(x, y, z)
    local d_radial = math.sqrt(x*x + y*y) - r
    local d_bottom = -z
    local d_top = z - h
    local d_vertical = math.max(d_bottom, d_top)
    local outside = math.sqrt(
      math.max(d_radial, 0)^2 +
      math.max(d_vertical, 0)^2
    )
    local inside = math.min(math.max(d_radial, d_vertical), 0)
    return outside + inside
  end
end

-- SDF for sphere (centered at origin)
local function sphere_sdf(r)
  return function(x, y, z)
    return math.sqrt(x*x + y*y + z*z) - r
  end
end

-- SDF for torus (centered at origin, hole along Z axis)
local function torus_sdf(major_radius, minor_radius)
  return function(x, y, z)
    local q = math.sqrt(x*x + y*y) - major_radius
    return math.sqrt(q*q + z*z) - minor_radius
  end
end

-- SDF for ring (annulus with height, base on XY plane at Z=0)
local function ring_sdf(inner_radius, outer_radius, height)
  return function(x, y, z)
    local rho = math.sqrt(x*x + y*y)
    local d_inner = inner_radius - rho
    local d_outer = rho - outer_radius
    local d_radial = math.max(d_inner, d_outer)
    local d_bottom = -z
    local d_top = z - height
    local d_vertical = math.max(d_bottom, d_top)
    local outside = math.sqrt(
      math.max(d_radial, 0)^2 +
      math.max(d_vertical, 0)^2
    )
    local inside = math.min(math.max(d_radial, d_vertical), 0)
    return outside + inside
  end
end

--- Create a box/cuboid (corner at origin)
-- @param w Width (X dimension)
-- @param d Depth (Y dimension), defaults to w
-- @param h Height (Z dimension), defaults to w
-- @return Shape object
function Primitives.box(w, d, h)
  d = d or w
  h = h or w

  return Shape(box_sdf(w, d, h),
    {min = {0, 0, 0}, max = {w, d, h}},
    {primitive = "box", params = {w = w, d = d, h = h}}
  )
end

--- Create a cylinder with base on XY plane, extending along +Z
-- @param r Radius
-- @param h Height
-- @return Shape object
function Primitives.cylinder(r, h)
  return Shape(cylinder_sdf(r, h),
    {min = {-r, -r, 0}, max = {r, r, h}},
    {primitive = "cylinder", params = {r = r, h = h}}
  )
end

--- Create a sphere centered at origin
-- @param r Radius
-- @return Shape object
function Primitives.sphere(r)
  return Shape(sphere_sdf(r),
    {min = {-r, -r, -r}, max = {r, r, r}},
    {primitive = "sphere", params = {r = r}}
  )
end

--- Create a torus centered at origin with hole along Z axis
-- @param major_radius Distance from center of torus to center of tube
-- @param minor_radius Radius of the tube
-- @return Shape object
function Primitives.torus(major_radius, minor_radius)
  local outer = major_radius + minor_radius
  return Shape(torus_sdf(major_radius, minor_radius),
    {min = {-outer, -outer, -minor_radius}, max = {outer, outer, minor_radius}},
    {primitive = "torus", params = {major_radius = major_radius, minor_radius = minor_radius}}
  )
end

--- Create a ring (annulus with height) with base on XY plane at Z=0
-- Used for coupling coils in inductive coupling applications
-- @param inner_radius Inner radius of the ring
-- @param outer_radius Outer radius of the ring
-- @param height Height of the ring along Z axis
-- @return Shape object
function Primitives.ring(inner_radius, outer_radius, height)
  return Shape(ring_sdf(inner_radius, outer_radius, height),
    {min = {-outer_radius, -outer_radius, 0}, max = {outer_radius, outer_radius, height}},
    {primitive = "ring", params = {inner_radius = inner_radius, outer_radius = outer_radius, h = height}}
  )
end

return Primitives
