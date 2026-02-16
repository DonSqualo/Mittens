-- Mittens Standard Library: ISO Metric Threads
-- Thread generation utilities for male, female, and intermediate ring threads

local Threads = {}

-- ISO metric thread specifications
-- Format: { major = diameter in mm, pitch_coarse = coarse pitch, pitch_fine = fine pitch }
Threads.ISO_SPECS = {
  M3 = { major = 3, pitch_coarse = 0.5, pitch_fine = 0.35 },
  M4 = { major = 4, pitch_coarse = 0.7, pitch_fine = 0.5 },
  M5 = { major = 5, pitch_coarse = 0.8, pitch_fine = 0.5 },
  M6 = { major = 6, pitch_coarse = 1.0, pitch_fine = 0.75 },
  M8 = { major = 8, pitch_coarse = 1.25, pitch_fine = 1.0 },
  M10 = { major = 10, pitch_coarse = 1.5, pitch_fine = 1.25 },
  M12 = { major = 12, pitch_coarse = 1.75, pitch_fine = 1.5 },
  M16 = { major = 16, pitch_coarse = 2.0, pitch_fine = 1.5 },
  M20 = { major = 20, pitch_coarse = 2.5, pitch_fine = 2.0 },
  M24 = { major = 24, pitch_coarse = 3.0, pitch_fine = 2.0 },
  M27 = { major = 27, pitch_coarse = 3, pitch_fine = 2 },
  M30 = { major = 30, pitch_coarse = 3.5, pitch_fine = 2.0 },
  M36 = { major = 36, pitch_coarse = 4.0, pitch_fine = 3.0 },
}

--- Create an external (male) ISO metric thread
-- @param params Table with: major_diameter (mm), pitch (mm), height (mm), segments_per_turn (optional, default 32), clearance (optional, mm)
-- @return Shape object representing the external thread
function Threads.external(params)
  local major_diameter = params.major_diameter or params.size_mm
  local pitch = params.pitch
  local height = params.height or 5
  local segments_per_turn = params.segments_per_turn or 32
  local clearance = params.clearance or 0

  if not major_diameter or not pitch then
    error("external_thread requires major_diameter and pitch")
  end

  local shape = {
    _type = "shape",
    _metadata = {
      primitive = "external_thread",
      params = {
        major_diameter = major_diameter,
        pitch = pitch,
        height = height,
        segments_per_turn = segments_per_turn,
        clearance = clearance,
      }
    },
    _ops = {},
    _bounds = {
      min = {-major_diameter/2, -major_diameter/2, 0},
      max = {major_diameter/2, major_diameter/2, height}
    }
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

--- Create an internal (female) ISO metric thread
-- @param params Table with: major_diameter (mm), pitch (mm), height (mm), segments_per_turn (optional, default 32)
-- @return Shape object representing the internal thread
function Threads.internal(params)
  local major_diameter = params.major_diameter or params.size_mm
  local pitch = params.pitch
  local height = params.height or 5
  local segments_per_turn = params.segments_per_turn or 32
  local clearance = params.clearance or 0
  local wall_thickness = params.wall_thickness

  if not major_diameter or not pitch then
    error("internal_thread requires major_diameter and pitch")
  end

  local shape = {
    _type = "shape",
    _metadata = {
      primitive = "internal_thread",
      params = {
        major_diameter = major_diameter,
        pitch = pitch,
        height = height,
        segments_per_turn = segments_per_turn,
        clearance = clearance,
        wall_thickness = wall_thickness,
      }
    },
    _ops = {},
    _bounds = {
      min = {-major_diameter/2, -major_diameter/2, 0},
      max = {major_diameter/2, major_diameter/2, height}
    }
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

--- Create an intermediate ring with external thread on outside and internal thread on inside
-- @param params Table with: size (e.g. "M27"), height (mm), wall_thickness (mm), segments_per_turn (optional)
-- @return Group with both external and internal thread components
function Threads.intermediate_ring(params)
  local size = params.size
  local height = params.height or 5
  local wall_thickness = params.wall_thickness or 2
  local segments_per_turn = params.segments_per_turn or 32

  if not size or not Threads.ISO_SPECS[size] then
    error("intermediate_ring requires valid size (e.g., 'M27')")
  end

  local spec = Threads.ISO_SPECS[size]
  local outer_diameter = spec.major
  local pitch = params.pitch or spec.pitch_coarse

  local thread_clearance = 0.25
  local inner_diameter = outer_diameter + thread_clearance

  local external = Threads.external({
    major_diameter = outer_diameter,
    pitch = pitch,
    height = height,
    segments_per_turn = segments_per_turn
  })
  :color(0.8, 0.8, 0.8, 1.0)
  :name(size .. " external thread")

  local internal = Threads.internal({
    major_diameter = inner_diameter,
    pitch = pitch,
    height = height,
    segments_per_turn = segments_per_turn
  })
  :at(0, 0, 0)
  :color(0.7, 0.7, 0.7, 1.0)
  :name(size .. " internal thread")

  local ring = {
    _type = "shape",
    _metadata = {
      primitive = "group",
      components = {external, internal},
      description = size .. " intermediate ring H" .. height
    },
    _ops = {},
    _material = nil,
    _bounds = {
      min = {-outer_diameter/2, -outer_diameter/2, 0},
      max = {outer_diameter/2, outer_diameter/2, height}
    },

    external_thread = external,
    internal_thread = internal,
  }

  setmetatable(ring, {__index = {
    serialize = function(self)
      return {
        type = "group",
        children = {
          self.external_thread:serialize(),
          self.internal_thread:serialize(),
        },
        name = self._metadata.description,
      }
    end,

    at = function(self, x, y, z)
      self.external_thread:at(x, y, z)
      self.internal_thread:at(x, y, z)
      return self
    end,

    color = function(self, r, g, b, a)
      self.external_thread:color(r, g, b, a)
      self.internal_thread:color(r, g, b, a)
      return self
    end,

    name = function(self, n)
      self._name = n
      return self
    end,
  }})

  return ring
end

return Threads
