-- hallbach.lua
-- Halbach array magnet holder system for cell culture studies
-- test change

local Mittens = require("stdlib")

-- ===========================
-- Configuration Parameters
-- ===========================

Config = {
  tolerance = 0.5,
  gap = 0.2,
  inch = 25.4,
}

Magnet = {
  size = 12.8,
  count_inner = 8,
  count_outer = 12,
}
Magnet.hypotenuse = Magnet.size * math.sqrt(2)

Coverslip = {
  radius = 20 / 2,
  height = 0.17 + 0.13,
}

Ring = {
  height = 30,
  center_hole_diameter = 25,
  rim = 0.4,
  platform_height = 0.5,
  overhang = 10,
  overhang_height = 2,
  vertical_offset = 5,
  total_hole_width = 110,
}
Ring.center_hole_radius = Ring.center_hole_diameter / 2
Ring.inner_radius = Ring.center_hole_radius + Magnet.hypotenuse + Ring.rim
Ring.outer_radius = Ring.inner_radius + Magnet.hypotenuse + 1.5
Ring.cutout_height = Ring.height - Config.inch
Ring.cutout_radius = Ring.center_hole_radius + Magnet.hypotenuse / 2
Ring.outer_cutout_radius = Ring.inner_radius + Magnet.hypotenuse / 2 + Config.gap

Cap = {
  height = 2,
}

Motor = {
  width = 42.3 + 0.2 + Ring.overhang_height,
  height = 38 + 0.4,
  wall = 5,
  insert_height = 1,
  screw_radius = 3.2 / 2,
  screw_offset = 31 / 2,
  axle_radius = 22 / 2 + 0.2,
  cable_insert = 16 + 2,
}

Belt = {
  offset = 3 + (Ring.vertical_offset + Motor.height + Motor.insert_height - Ring.height),
  width = 6 + 1,
}
Belt.height = 1 + Belt.width + Belt.offset

Well = {
  wall = 1.1,
  height = 5,
  liquid_wall = 1,
}
Well.deep_height = Ring.height + Ring.platform_height + Cap.height + 2 * Well.liquid_wall
Well.deep_radius = Coverslip.radius + Config.gap + Well.wall
Well.cutout = Coverslip.radius + Config.gap + Well.wall + Config.tolerance
Well.bottom_wall = Well.liquid_wall * 2
Well.cutout_height = 3 * Well.height

Setter = {
  width = 4 * Well.liquid_wall + Ring.center_hole_radius,
  height = 5,
  count = 5,
  offset = 2,
}
Setter.angle = 70 / Setter.count

Transducer = {
  diameter = 64,
  height = 5,
  radius = 10 / 2,
  trans_height = 4,
  position = 5 + 1,
  offset = 0.2,
  focal_distance = 50,
}
Transducer.holder_width = Transducer.radius + Config.tolerance + Well.wall

Assembly = {
  total_height = Ring.height + Cap.height,
}
Assembly.top_container_offset = Transducer.focal_distance - Assembly.total_height

Cable = {
  width = 4,
}

-- ===========================
-- Helper: Oval shape
-- ===========================

local function oval(r, w, h)
  return union(
    cylinder(r, h):at(w, 0, 0),
    cylinder(r, h):at(-w, 0, 0),
    box(2 * w, 2 * r, h):centerXY():at(0, 0, h / 2)
  )
end

-- ===========================
-- Geometry: Ring Cylinder (Inner)
-- ===========================

local ring_body = difference(
  cylinder(Ring.inner_radius, Ring.height),
  cylinder(Ring.center_hole_radius, Ring.height + 1):at(0, 0, -0.5)
)

local inner_magnet_cutouts = {}
for i = 0, Magnet.count_inner - 1 do
  local angle = i * (360 / Magnet.count_inner)
  local is_rotated = (i % 2 == 1)
  local rotation = is_rotated and 45 or 0
  table.insert(inner_magnet_cutouts,
    box(Magnet.size, Magnet.size, Ring.height + 1):centered()
    :rotate(0, 0, rotation)
    :at(Ring.cutout_radius, 0, Ring.height / 2)
    :rotate(0, 0, angle)
  )
end

local inner_slot_cutouts = {}
for i = 0, Magnet.count_inner - 1 do
  local angle = i * (360 / Magnet.count_inner)
  local is_rotated = (i % 2 == 1)
  local rotation = is_rotated and 45 or 0
  table.insert(inner_slot_cutouts,
    box(Magnet.size / 2, Magnet.size + 2, Ring.cutout_height):centered()
    :rotate(0, 0, rotation)
    :at(Ring.cutout_radius, 0, Ring.height - Ring.cutout_height / 2)
    :rotate(0, 0, angle)
  )
end

RingInner = {}
RingInner.body = difference(
  ring_body,
  { inner_magnet_cutouts,
    inner_slot_cutouts }
)
-- RingInner.body = difference(RingInner.body, inner_slot_cutouts)

RingInner.platform = difference(
  cylinder((Ring.total_hole_width - Config.gap) / 2, Ring.platform_height):at(0, 0, -Ring.platform_height),
  cylinder(Ring.center_hole_radius, Ring.height + 1):at(0, 0, -Ring.platform_height - 1)
)

RingInner.overhang = difference(
  cylinder((Ring.total_hole_width + Ring.overhang) / 2, Ring.vertical_offset + Ring.overhang_height),
  cylinder(Ring.outer_radius + 1, Ring.vertical_offset + Ring.overhang_height),
  difference(
    cylinder(Ring.total_hole_width, Ring.vertical_offset),
    cylinder((Ring.total_hole_width - Config.gap) / 2, Ring.vertical_offset)
  )
)

-- ===========================
-- Geometry: Ring Cylinder (Outer)
-- ===========================

local outer_ring_body = difference(
  cylinder(Ring.outer_radius, Ring.height),
  cylinder(Ring.inner_radius + Config.gap, Ring.height + 1):at(0, 0, -0.5)
)

local outer_magnet_cutouts = {}
for i = 0, Magnet.count_outer - 1 do
  local angle = i * (360 / Magnet.count_outer)
  local rotation = (i % 3 == 0) and 0 or ((i % 3 == 1) and 30 or -30)
  table.insert(outer_magnet_cutouts,
    box(Magnet.size, Magnet.size, Ring.height + 1):centered()
    :rotate(0, 0, rotation)
    :at(Ring.outer_cutout_radius, 0, Ring.height / 2)
    :rotate(0, 0, angle)
  )
end

local outer_slot_cutouts = {}
for i = 0, Magnet.count_outer - 1 do
  local angle = i * (360 / Magnet.count_outer)
  local rotation = (i % 3 == 0) and 0 or ((i % 3 == 1) and 30 or -30)
  table.insert(outer_slot_cutouts,
    box(Magnet.size / 2, Magnet.size + 2, Ring.cutout_height * 2):centered()
    :rotate(0, 0, rotation)
    :at(Ring.outer_cutout_radius, 0, Ring.height - Ring.cutout_height / 2)
    :rotate(0, 0, angle)
  )
end

RingOuter = {}
RingOuter.body = difference(outer_ring_body, outer_magnet_cutouts)
RingOuter.body = difference(RingOuter.body, outer_slot_cutouts)

-- ===========================
-- Geometry: Cap Inner
-- ===========================

CapInner = {}

CapInner.top = difference(
  cylinder(Ring.inner_radius, Cap.height):at(0, 0, Ring.height),
  cylinder(Ring.center_hole_radius, Cap.height + 1):at(0, 0, Ring.height - 0.2)
)

local cap_inner_magnets = {}
for i = 0, Magnet.count_inner - 1 do
  local angle_deg = i * (360 / Magnet.count_inner)
  local angle_rad = math.rad(angle_deg)
  local is_rotated = (i % 2 == 1)
  local rotation = is_rotated and 45 or 0
  local x = Ring.cutout_radius * math.cos(angle_rad)
  local y = Ring.cutout_radius * math.sin(angle_rad)
  table.insert(cap_inner_magnets,
    box(Magnet.size, Magnet.size, Ring.cutout_height):centered()
    :rotate(0, 0, rotation + angle_deg)
    :at(x, y, Ring.height - Ring.cutout_height / 2)
  )
end

local cap_inner_slots = {}
for i = 0, Magnet.count_inner - 1 do
  local angle_deg = i * (360 / Magnet.count_inner)
  local angle_rad = math.rad(angle_deg)
  local is_rotated = (i % 2 == 1)
  local rotation = is_rotated and 45 or 0
  local x = Ring.cutout_radius * math.cos(angle_rad)
  local y = Ring.cutout_radius * math.sin(angle_rad)
  table.insert(cap_inner_slots,
    box(Magnet.size / 2, Magnet.size + 2 - Config.gap, Ring.cutout_height):centered()
    :rotate(0, 0, rotation + angle_deg)
    :at(x, y, Ring.height - Ring.cutout_height / 2)
  )
end

CapInner.model = group("cap_inner", { CapInner.top }):color(0.3, 0.6, 0.9)
for _, v in ipairs(cap_inner_magnets) do CapInner.model:add(v) end
for _, v in ipairs(cap_inner_slots) do CapInner.model:add(v) end

-- ===========================
-- Geometry: Cap Outer
-- ===========================

CapOuter = {}
CapOuter.body = difference(
  cylinder(Ring.outer_radius, Belt.height):at(0, 0, Ring.height),
  cylinder(Ring.inner_radius + Config.gap, Belt.height):at(0, 0, Ring.height)
)

local cap_outer_magnets = {}
for i = 0, Magnet.count_outer - 1 do
  local angle_deg = i * (360 / Magnet.count_outer)
  local angle_rad = math.rad(angle_deg)
  local rotation = (i % 3 == 0) and 0 or ((i % 3 == 1) and 30 or -30)
  local x = Ring.outer_cutout_radius * math.cos(angle_rad)
  local y = Ring.outer_cutout_radius * math.sin(angle_rad)
  table.insert(cap_outer_magnets,
    box(Magnet.size - Config.tolerance, Magnet.size - Config.tolerance, Ring.cutout_height - Config.tolerance):center(
      true, true, true)
    :rotate(0, 0, rotation + angle_deg)
    :at(x, y, Ring.height - Ring.cutout_height / 2 + Config.tolerance)
  )
end

local cap_outer_slots = {}
for i = 0, Magnet.count_outer - 1 do
  local angle_deg = i * (360 / Magnet.count_outer)
  local angle_rad = math.rad(angle_deg)
  local rotation = (i % 3 == 0) and 0 or ((i % 3 == 1) and 30 or -30)
  local x = Ring.outer_cutout_radius * math.cos(angle_rad)
  local y = Ring.outer_cutout_radius * math.sin(angle_rad)
  table.insert(cap_outer_slots,
    box(Magnet.size / 2 - Config.tolerance, Magnet.size + 2 - Config.tolerance, Ring.cutout_height - Config.tolerance)
    :centered()
    :rotate(0, 0, rotation + angle_deg)
    :at(x, y, Ring.height - Ring.cutout_height / 2 + Config.tolerance)
  )
end

CapOuter.model = group("cap_outer", { CapOuter.body })
for _, v in ipairs(cap_outer_magnets) do CapOuter.model:add(v) end
for _, v in ipairs(cap_outer_slots) do CapOuter.model:add(v) end

-- ===========================
-- Geometry: Well
-- ===========================

Well.model = difference(
  cylinder(Coverslip.radius + Config.gap + Well.wall, Well.height),
  cylinder(Coverslip.radius + Config.gap, Well.height):at(0, 0, Well.wall / 2),
  cylinder(Coverslip.radius - Well.wall, Well.height / 2)
)

-- ===========================
-- Geometry: Deep Well
-- ===========================

DeepWell = {}
DeepWell.body = difference(
  cylinder(Well.deep_radius, Well.deep_height):at(0, 0, -Ring.platform_height),
  cylinder(Coverslip.radius + Config.gap, Well.deep_height):at(0, 0, -Ring.platform_height + Well.wall / 2),
  cylinder(Coverslip.radius - 2 * Well.wall, Well.height / 2):at(0, 0, -Ring.platform_height)
)

DeepWell.top = difference(
  cylinder(Well.deep_radius + 2 * Well.liquid_wall, Well.liquid_wall):at(0, 0, Well.deep_height - Ring.platform_height),
  cylinder(Coverslip.radius + Config.gap, 2 * Well.liquid_wall):at(0, 0, Well.deep_height - Ring.platform_height)
)

DeepWell.model = group("deep_well", { DeepWell.body, DeepWell.top })

-- ===========================
-- Geometry: Well Holder
-- ===========================

WellHolder = {}

WellHolder.wall = difference(
  cylinder(Ring.center_hole_radius - Config.tolerance, Assembly.total_height),
  cylinder(Ring.center_hole_radius - Well.liquid_wall, Assembly.total_height + Well.liquid_wall),
  box(Well.cutout * 2, Ring.center_hole_radius, Well.cutout_height):at(-Well.cutout, 0, 0)
)

WellHolder.bottom = difference(
  cylinder(Ring.center_hole_radius - Config.tolerance, Well.bottom_wall):at(0, 0, -Well.bottom_wall),
  cylinder(Coverslip.radius - Well.wall, Well.bottom_wall):at(0, 0, -Well.bottom_wall),
  cylinder(Coverslip.radius + Config.gap + Well.wall, Well.height):at(0, 0, -2 * Config.tolerance)
)

WellHolder.top_ring = difference(
  cylinder(2 * Well.liquid_wall + Ring.center_hole_radius, Well.liquid_wall):at(0, 0, Assembly.total_height),
  cylinder(Transducer.holder_width, Well.liquid_wall):at(0, 0, Assembly.total_height)
)

local setter_slots = {}
for i = 0, Setter.count * 2 - 1 do
  local angle = -5 + Setter.angle * i
  table.insert(setter_slots,
    box(Setter.width * 2, Setter.width / 3, Setter.height * 4):centered()
    :at(0, 0, i * Setter.offset)
    :rotate(0, 0, angle)
  )
end

WellHolder.setter = difference(
  cylinder(2 * Well.liquid_wall + Ring.center_hole_radius, Setter.height + Setter.count * Setter.offset):at(0, 0,
    Assembly.total_height),
  cylinder(Ring.center_hole_radius - Well.liquid_wall, Assembly.total_height):at(0, 0, Assembly.total_height),
  group(setter_slots):at(0, 0, Assembly.total_height + Setter.height * 2 + Well.liquid_wall)
)

WellHolder.model = group("well_holder", {
  WellHolder.wall,
  WellHolder.bottom,
  WellHolder.top_ring,
  WellHolder.setter
})

-- ===========================
-- Geometry: Deep Well Holder
-- ===========================

DeepWellHolder = {}

DeepWellHolder.top_ring = difference(
  cylinder(Setter.width, Well.liquid_wall):at(0, 0, Assembly.total_height),
  cylinder(Well.deep_radius + Config.tolerance, Well.liquid_wall):at(0, 0, Assembly.total_height)
)

local deep_setter_slots = {}
for i = 0, Setter.count * 2 - 1 do
  local angle = -5 + Setter.angle * i
  table.insert(deep_setter_slots,
    box(Setter.width * 2, Setter.width / 3, Setter.height * 4):centered()
    :at(0, 0, i * Setter.offset)
    :rotate(0, 0, angle)
  )
end

DeepWellHolder.setter = difference(
  cylinder(Setter.width, Setter.height + Setter.count * Setter.offset):at(0, 0, Assembly.total_height + Well.liquid_wall),
  cylinder(Ring.center_hole_radius + 2 * Well.liquid_wall, Assembly.total_height):at(0, 0,
    Assembly.total_height + Well.liquid_wall),
  group(deep_setter_slots):at(0, 0, Assembly.total_height + Well.liquid_wall + Setter.height * 2 + Well.liquid_wall)
)

DeepWellHolder.model = group("deep_well_holder", { DeepWellHolder.top_ring, DeepWellHolder.setter })

-- ===========================
-- Geometry: Transducer
-- ===========================

Transducer.model = cylinder(Transducer.radius, Transducer.trans_height):at(0, 0, Transducer.position + 0.1)

-- ===========================
-- Geometry: Transducer Holder
-- ===========================

TransHolder = {}

TransHolder.body = difference(
  cylinder(Transducer.holder_width, Well.liquid_wall - Config.tolerance + Assembly.total_height - Transducer.offset):at(
    0, 0, Transducer.offset + Config.tolerance),
  cylinder(Transducer.radius + Config.tolerance, Transducer.trans_height + Config.tolerance / 2):at(0, 0,
    Transducer.offset + Config.tolerance),
  box(Cable.width, 3 * Well.wall, Assembly.total_height + Config.tolerance):at(-Cable.width / 2,
    Transducer.holder_width - 3 * Well.wall, Transducer.offset + Config.tolerance)
)

local trans_setter_bars = {}
for i = 0, Setter.count - 1 do
  local angle = Setter.angle * i
  table.insert(trans_setter_bars,
    box(Setter.width * 2, Setter.width / 4, Setter.height):centered()
    :at(0, 0, i * Setter.offset)
    :rotate(0, 0, angle)
  )
end

TransHolder.setter = group("trans_setter", trans_setter_bars)
    :at(0, 0, Assembly.total_height + Setter.height / 2 + Well.liquid_wall)

TransHolder.model = group("trans_holder", { TransHolder.body, TransHolder.setter })

-- ===========================
-- Motor Holder
-- ===========================

MotorHolder = {}

local motor_screw_holes = {}
for x = -1, 1, 2 do
  for y = -1, 1, 2 do
    table.insert(motor_screw_holes,
      oval(Motor.screw_radius, 1, 2 * Ring.height):at(x * Motor.screw_offset + 1, y * Motor.screw_offset, 0))
  end
end

MotorHolder.model = difference(
  box(Motor.width + 2 * Motor.wall, Motor.width + 2 * Motor.wall, Motor.height + Motor.insert_height):center(true, true,
    true),
  box(Motor.width, Motor.width, Motor.height):centered():at(0, 0, -Motor.insert_height),
  motor_screw_holes,
  oval(Motor.axle_radius, 2, 2 * Motor.height):at(1, 0, 0),
  box(Motor.width, Motor.width, Motor.height):centered():at(Motor.width / 2, 0, -Motor.insert_height)
)

-- ===========================
-- Assembly: Ring Cylinder with Motor Holder
-- ===========================

RingInner.motor = MotorHolder.model
    :at((Ring.total_hole_width + Motor.width + Ring.overhang) / 2, 0,
      Ring.vertical_offset + (Motor.height + Motor.insert_height) / 2)

RingInner.model = group("ring_inner", {
  RingInner.body,
  RingInner.platform,
  RingInner.overhang,
  RingInner.motor
})

-- ===========================
-- Magnet Insert Helper
-- ===========================

MagnetInsertHelper = {}
MagnetInsertHelper.model = difference(
  union(
    cylinder(Ring.outer_radius, Cap.height),
    box(Magnet.size / 2, Magnet.size + 1.8, Ring.cutout_height):centered():at(0, 0, -Ring.cutout_height / 2)
  ),
  box(Magnet.size, Magnet.size, Ring.height + 1):centered()
)

-- ===========================
-- FUS Holder (Focused Ultrasound)
-- ===========================

FUSHolder = {}

FUSHolder.tube = difference(
  cylinder(Ring.center_hole_radius, Assembly.total_height),
  cylinder(Ring.center_hole_radius - Well.liquid_wall, Assembly.total_height + Well.liquid_wall)
)

FUSHolder.top = difference(
  cylinder(Well.liquid_wall + Transducer.diameter / 2, Assembly.top_container_offset + Transducer.height):at(0, 0,
    Assembly.total_height),
  cylinder((Transducer.diameter / 2) + 2 * Well.liquid_wall, Assembly.top_container_offset):at(0, 0,
    Assembly.total_height),
  cylinder(Transducer.diameter / 2, Transducer.height):at(0, 0, Assembly.total_height + Assembly.top_container_offset)
)

FUSHolder.flange = difference(
  cylinder(2 * Well.liquid_wall + Ring.center_hole_radius, 0.6 * Well.liquid_wall):at(0, 0, Assembly.total_height),
  cylinder(Ring.center_hole_radius - Well.liquid_wall, Well.liquid_wall):at(0, 0, Assembly.total_height)
)

FUSHolder.bottom = difference(
  cylinder(Ring.center_hole_radius, Well.liquid_wall):at(0, 0, -Well.liquid_wall),
  cylinder(Coverslip.radius - Well.liquid_wall / 2, Well.liquid_wall):at(0, 0, -Well.liquid_wall),
  cylinder(Coverslip.radius, Coverslip.height):at(0, 0, -Well.liquid_wall)
)

FUSHolder.model = group("fus_holder", {
  FUSHolder.tube,
  FUSHolder.top,
  FUSHolder.flange,
  FUSHolder.bottom
})

DeepWellRing = {
  height = 13,
  width = 10
}

DeepWellRing.model = difference(
  cylinder(Well.deep_radius + DeepWellRing.width, DeepWellRing.height),
  cylinder(Well.deep_radius, DeepWellRing.height)
):at(0, 0, Assembly.total_height)

-- ===========================
-- Geometry: Zeiss Plate Holder Adapter
-- Fits standard 96-well plate holder dimensions
-- ===========================

Microscope = {
  height = 6,
  WellPlate = {
    length = 127.76,
    width = 85.48,
  }
}
Microscope.length = Microscope.WellPlate.length + 20
Microscope.width = Microscope.WellPlate.width + 20
Microscope.WellPlate.offset = Microscope.height - 2

Microscope.model = difference(
  box(Microscope.length, Microscope.width, Microscope.height):centerXY(),
  box(Microscope.WellPlate.length, Microscope.WellPlate.width, Microscope.height):centerXY():at(0, 0,
    Microscope.height - Microscope.WellPlate.offset),
  box(Microscope.WellPlate.length - Microscope.WellPlate.offset,
    Microscope.WellPlate.width - Microscope.WellPlate.offset,
    Microscope.height):centerXY()
):at(0, 0, -Microscope.height - Ring.platform_height)
  :material(material("steel"))

HolderAdapter = {
  length = 128,
  width = 86,
  tolerance = 0.5,
  height = 8,
  offset = 1,
}

-- Cutout sized for the Halbach inner ring platform
HolderAdapter.cutout_radius = (Ring.total_hole_width + Config.gap) / 2

HolderAdapter.model = difference(
  box(HolderAdapter.length, HolderAdapter.width, HolderAdapter.height):centerXY(),
  cylinder(HolderAdapter.cutout_radius, HolderAdapter.height):at(0, 0, HolderAdapter.offset),
  cylinder(Ring.center_hole_radius, HolderAdapter.height)
):at(0, 0, -HolderAdapter.height - Ring.platform_height)
-- ===========================
-- Active Model
-- ===========================

local assembly = group("assembly", {
  RingInner.model,
  -- RingOuter.body,
  -- DeepWell.model:at(0, 0, 18),
  DeepWellRing.model:color(1, 0, 0),
  CapInner.model,
  Microscope.model,
  HolderAdapter.model,
})

Mittens.register(assembly)

export_stl("cap_inner.stl", CapInner.model)
export_stl("tight_fit_adapter.stl", DeepWellRing.model)
export_stl("microscope_holder.stl", Microscope.model)
export_stl("holder_adapter.stl", HolderAdapter.model)

-- ===========================
-- View Configuration
-- ===========================

-- Camera at distance ~280 from target (0,0,5), isometric angle
-- Direction vector (1,1,1) normalized * 280 ≈ (162, 162, 162) + target offset
view({
  camera = {
    position = { 160, 160, 165 },
    target = { 0, 0, 5 },
  },
  theme = "dark",
  axes = { show = true, size = 20 },
})

return Mittens.serialize()
