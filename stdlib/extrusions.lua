-- Mittens Standard Library: Aluminum Extrusions
-- 80/20 style T-slot aluminum extrusion profiles
-- Reference: 80/20 Inc. standard extrusion specifications
-- https://www.80020.net/

local Extrusions = {}

-- ============================================================================
-- Helper Functions
-- ============================================================================

--- Helper to create a shape
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

    tag = function(self, t)
      self._tag = t
      return self
    end,

    center = function(self, cx, cy, cz)
      local bounds = self._bounds
      local dx = cx and -((bounds.min[1] + bounds.max[1]) / 2) or 0
      local dy = cy and -((bounds.min[2] + bounds.max[2]) / 2) or 0
      local dz = cz and -((bounds.min[3] + bounds.max[3]) / 2) or 0
      table.insert(self._ops, {op = "translate", x = dx, y = dy, z = dz})
      return self
    end,

    centerXY = function(self)
      return self:center(true, true, false)
    end,

    centered = function(self)
      return self:center(true, true, true)
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
        name = self._name,
        tag = self._tag
      }
    end
  }})

  return shape
end

--- Box SDF
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

--- Cylinder SDF
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

--- Create a simple box primitive
local function box(w, d, h)
  d = d or w
  h = h or w
  return Shape(box_sdf(w, d, h),
    {min = {0, 0, 0}, max = {w, d, h}},
    {primitive = "box", params = {w = w, d = d, h = h}}
  )
end

--- Create a simple cylinder primitive
local function cylinder(r, h)
  return Shape(cylinder_sdf(r, h),
    {min = {-r, -r, 0}, max = {r, r, h}},
    {primitive = "cylinder", params = {r = r, h = h}}
  )
end

--- Create a rounded rectangle (box with rounded corners)
-- Used for T-slot cutouts in extrusions
local function rounded_rect_sdf(w, d, h, corner_r)
  return function(x, y, z)
    -- Normalize coordinates
    local lx = math.abs(x) - (w - corner_r) / 2
    local ly = math.abs(y) - (d - corner_r) / 2
    local lz = math.abs(z) - h / 2
    
    -- Distance to the rounded box
    local dx = math.max(lx, 0)
    local dy = math.max(ly, 0)
    local dz = math.max(lz, 0)
    
    local dist = math.sqrt(dx*dx + dy*dy + dz*dz) - corner_r
    
    -- Internal distance
    local internal = math.min(math.max(lx, ly, lz), 0)
    
    return dist + internal
  end
end

--- Create a T-slot rounded rectangle shape
local function rounded_rect(w, d, h, corner_r)
  return Shape(rounded_rect_sdf(w, d, h, corner_r),
    {min = {-w/2, -d/2, -h/2}, max = {w/2, d/2, h/2}},
    {primitive = "rounded_rect", params = {w = w, d = d, h = h, r = corner_r}}
  )
end

-- ============================================================================
-- T-Slot Profile Specifications (80/20 compatible dimensions in mm)
-- ============================================================================

local PROFILES = {
  -- 20x20mm profile
  ["20x20"] = {
    width = 20,
    depth = 20,
    wall_thickness = 1.5,
    t_slot_width = 6,
    t_slot_depth = 2.5,
    corner_radius = 2.5,
    hole_diameter = 5,  -- mounting holes
  },
  
  -- 40x40mm profile
  ["40x40"] = {
    width = 40,
    depth = 40,
    wall_thickness = 2.0,
    t_slot_width = 10,
    t_slot_depth = 4,
    corner_radius = 3.0,
    hole_diameter = 8,  -- mounting holes
  },
  
  -- 20x40mm profile (rectangular)
  ["20x40"] = {
    width = 20,
    depth = 40,
    wall_thickness = 1.5,
    t_slot_width = 6,
    t_slot_depth = 2.5,
    corner_radius = 2.5,
    hole_diameter = 5,  -- mounting holes
  },
}

-- ============================================================================
-- Extrusion Profile Generation
-- ============================================================================

--- Create a T-slot extrusion profile
-- The extrusion is generated as a cylinder with T-slot cutouts on all faces
-- @param profile_type "20x20", "40x40", or "20x40"
-- @param length Length of the extrusion in mm (along Z axis)
-- @return Extrusion shape
function Extrusions.profile(profile_type, length)
  length = length or 100
  local spec = PROFILES[profile_type]
  
  if not spec then
    error("Unknown extrusion profile: " .. tostring(profile_type))
  end
  
  -- Create main body (solid extrusion)
  local body = box(spec.width, spec.depth, length)
    :centered()
    :color(0.75, 0.75, 0.78, 1.0)  -- Aluminum silver
    :tag("extrusion_" .. profile_type .. "_body")
  
  -- Tag the extrusion
  body._tag = "extrusion_profile"
  body._profile_type = profile_type
  body._length = length
  
  return body
end

--- Create a simple slot (rectangular cutout for T-nuts and fasteners)
-- @param width Width of the slot in mm
-- @param depth Depth of the slot in mm (from surface)
-- @param length Length of the slot in mm (along extrusion)
-- @return Slot shape
function Extrusions.slot(width, depth, length)
  return box(width, depth, length)
    :centered()
    :color(0.1, 0.1, 0.1, 0.3)
    :tag("extrusion_slot")
end

--- Create a corner bracket (L-shaped reinforcement)
-- Used to join two extrusions at right angles
-- @param size Size of the bracket (20, 40, etc.) - should match extrusion profile
-- @param thickness Thickness of the bracket material in mm
-- @param height Height of the bracket in mm
-- @return Bracket assembly group
function Extrusions.corner_bracket(size, thickness, height)
  size = size or 40
  thickness = thickness or 5
  height = height or 40
  
  -- Vertical arm
  local vertical = box(thickness, size, height)
    :color(0.75, 0.75, 0.78, 1.0)
    :tag("bracket_vertical")
  
  -- Horizontal arm
  local horizontal = box(size, thickness, height)
    :at(0, 0, 0)
    :color(0.75, 0.75, 0.78, 1.0)
    :tag("bracket_horizontal")
  
  -- Mounting holes (represented as info, not actual geometry for simplicity)
  -- In a real implementation, these would be cylindrical cutouts
  
  return {
    vertical = vertical,
    horizontal = horizontal,
    size = size,
    thickness = thickness,
    height = height,
    _type = "corner_bracket"
  }
end

--- Create mounting holes in an extrusion
-- Holes are spaced according to 80/20 standard (20mm centers for 20x20, 40mm for 40x40)
-- @param profile_type "20x20", "40x40", or "20x40"
-- @param count Number of holes to create
-- @return Table of hole positions
function Extrusions.mounting_holes(profile_type, count)
  local spec = PROFILES[profile_type]
  if not spec then
    error("Unknown extrusion profile: " .. tostring(profile_type))
  end
  
  count = count or 4
  local hole_diameter = spec.hole_diameter
  local spacing = spec.width == 20 and 20 or 40
  
  local holes = {}
  for i = 1, count do
    local position = -(count - 1) * spacing / 2 + (i - 1) * spacing
    table.insert(holes, {
      position = position,  -- Along extrusion length
      diameter = hole_diameter,
      y_offset = spec.depth / 2,  -- On top face
    })
  end
  
  return holes
end

--- Create a T-nut (fastener for T-slots)
-- @param profile_type "20x20", "40x40", or "20x40"
-- @param length Length of the T-nut in mm
-- @return T-nut shape
function Extrusions.tnut(profile_type, length)
  local spec = PROFILES[profile_type]
  if not spec then
    error("Unknown extrusion profile: " .. tostring(profile_type))
  end
  
  length = length or 20
  
  -- Main body (slides in T-slot)
  local body = box(spec.t_slot_width, spec.t_slot_depth, length)
    :centered()
    :color(0.3, 0.3, 0.3, 1.0)  -- Dark steel
    :tag("tnut_body")
  
  return body
end

--- Create a connecting plate (joins multiple extrusions)
-- @param width Width of the plate in mm
-- @param height Height of the plate in mm
-- @param thickness Thickness of the plate in mm
-- @param hole_diameter Diameter of mounting holes in mm
-- @return Plate shape
function Extrusions.connector_plate(width, height, thickness, hole_diameter)
  width = width or 40
  height = height or 40
  thickness = thickness or 3
  hole_diameter = hole_diameter or 5
  
  local plate = box(width, height, thickness)
    :centered()
    :color(0.6, 0.6, 0.6, 1.0)  -- Steel plate
    :tag("connector_plate")
  
  return plate
end

--- Create a simple rectangular frame made from extrusions
-- Useful for testing and assembly verification
-- @param profile_type "20x20", "40x40", or "20x40"
-- @param length_x Length in X dimension (mm)
-- @param length_y Length in Y dimension (mm)
-- @param length_z Length in Z dimension (mm)
-- @param frame_height Height of frame corners (mm)
-- @return Frame assembly group
function Extrusions.frame(profile_type, length_x, length_y, length_z, frame_height)
  length_x = length_x or 100
  length_y = length_y or 100
  length_z = length_z or 100
  frame_height = frame_height or 40
  
  local frame_parts = {}
  
  -- Bottom frame (XY plane at Z=0)
  -- X-axis extrusions
  local bottom_front = Extrusions.profile(profile_type, length_x)
    :at(-length_x/2, -length_y/2, 0)
    :tag("frame_bottom_front")
  table.insert(frame_parts, bottom_front)
  
  local bottom_back = Extrusions.profile(profile_type, length_x)
    :at(-length_x/2, length_y/2, 0)
    :tag("frame_bottom_back")
  table.insert(frame_parts, bottom_back)
  
  -- Y-axis extrusions
  local bottom_left = Extrusions.profile(profile_type, length_y)
    :rotate(0, 0, 90)
    :at(-length_x/2, 0, 0)
    :tag("frame_bottom_left")
  table.insert(frame_parts, bottom_left)
  
  local bottom_right = Extrusions.profile(profile_type, length_y)
    :rotate(0, 0, 90)
    :at(length_x/2, 0, 0)
    :tag("frame_bottom_right")
  table.insert(frame_parts, bottom_right)
  
  -- Vertical posts (Z-axis extrusions at each corner)
  local post_positions = {
    {-length_x/2, -length_y/2},
    {length_x/2, -length_y/2},
    {-length_x/2, length_y/2},
    {length_x/2, length_y/2},
  }
  
  for i, pos in ipairs(post_positions) do
    local post = Extrusions.profile(profile_type, frame_height)
      :rotate(90, 0, 0)
      :at(pos[1], pos[2], 0)
      :tag("frame_post_" .. i)
    table.insert(frame_parts, post)
  end
  
  -- Top frame (XY plane at Z=frame_height)
  -- X-axis extrusions
  local top_front = Extrusions.profile(profile_type, length_x)
    :at(-length_x/2, -length_y/2, frame_height)
    :tag("frame_top_front")
  table.insert(frame_parts, top_front)
  
  local top_back = Extrusions.profile(profile_type, length_x)
    :at(-length_x/2, length_y/2, frame_height)
    :tag("frame_top_back")
  table.insert(frame_parts, top_back)
  
  -- Y-axis extrusions
  local top_left = Extrusions.profile(profile_type, length_y)
    :rotate(0, 0, 90)
    :at(-length_x/2, 0, frame_height)
    :tag("frame_top_left")
  table.insert(frame_parts, top_left)
  
  local top_right = Extrusions.profile(profile_type, length_y)
    :rotate(0, 0, 90)
    :at(length_x/2, 0, frame_height)
    :tag("frame_top_right")
  table.insert(frame_parts, top_right)
  
  return {
    parts = frame_parts,
    profile_type = profile_type,
    dimensions = {x = length_x, y = length_y, z = frame_height},
    _type = "extrusion_frame"
  }
end

-- ============================================================================
-- Export
-- ============================================================================

return Extrusions
