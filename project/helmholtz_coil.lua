-- helmholtz_coil.lua
-- Helmholtz coil pair for uniform magnetic field generation

local Mittens = require("stdlib")

-- ===========================
-- Configuration Parameters
-- ===========================

Config = {
  domain_radius = 200,
  domain_height = 300,
}

Wire = {
  diameter = 0.8,
  packing_factor = 0.82,
}
Wire.pitch = Wire.diameter / Wire.packing_factor

Coil = {
  mean_radius = 50,
  gap = 42.5,
  windings = 120,
  layers = 12,
  current = 2.0,
}
Coil.turns_per_layer = math.ceil(Coil.windings / Coil.layers)
Coil.width = Coil.turns_per_layer * Wire.pitch
Coil.height = Coil.layers * Wire.pitch
Coil.inner_radius = Coil.mean_radius - Coil.height / 2
Coil.outer_radius = Coil.mean_radius + Coil.height / 2
Coil.right_x = Coil.gap / 2 + Coil.width / 2
Coil.left_x = -Coil.gap / 2 - Coil.width / 2
Coil.center_distance = Coil.gap + Coil.width

Resonator = {
  wall = 2.0,
  height = 90,
  gap = 0.8
}
Resonator.outer_radius = Coil.gap / 2
Resonator.inner_radius = Resonator.outer_radius - Resonator.wall

Bridge = {
  width = 20,
  distance = 1,
  thickness = 2
}
Bridge.outer_radius = Resonator.outer_radius + Bridge.thickness + Bridge.distance

CouplingCoil = {
  turns = 1,
  radius = Resonator.outer_radius,
  wire_diameter = 1.5,
  distance = 5.0,
  resistance = 0.1,
}
CouplingCoil.z_position = Resonator.height / 2 + CouplingCoil.distance

Scaffold = {
  clearance = 1.0,
  bridge_clearance = 2.0,
  axial_hole_radius = 5,
  stopper_width = 3.0,
  box_wall = 5.0,
}
Scaffold.tube_radius = Coil.inner_radius - Scaffold.clearance
Scaffold.tube_length = Coil.gap + 2 * Coil.width + 2 * Scaffold.stopper_width
Scaffold.stopper_radius = Coil.outer_radius
Scaffold.box_length = 2 * Bridge.outer_radius + 2 * Scaffold.box_wall
Scaffold.box_height = Scaffold.stopper_radius

-- ===========================
-- Materials
-- ===========================

local copper = material("copper", {
  conductivity = 5.8e7,
  relative_permeability = 1.0,
})

local air = material("air", {
  relative_permeability = 1.0,
})

local ptfe = material("ptfe", {
  relative_permittivity = 2.1,
  relative_permeability = 1.0,
})

local dark_pla = material("pla", {
  relative_permittivity = 3.0,
  relative_permeability = 1.0,
})

-- ===========================
-- Geometry: Right Coil
-- ===========================

RightCoil = {}
RightCoil.body = difference(
  cylinder(Coil.outer_radius, Coil.width),
  cylinder(Coil.inner_radius, Coil.width + 1)
):centered():rotate(0, 90, 0):at(Coil.right_x, 0, 0):material(copper)

-- ===========================
-- Geometry: Left Coil
-- ===========================

LeftCoil = {}
LeftCoil.body = difference(
  cylinder(Coil.outer_radius, Coil.width),
  cylinder(Coil.inner_radius, Coil.width + 1)
):centered():rotate(0, 90, 0):at(Coil.left_x, 0, 0):material(copper)

-- ===========================
-- Geometry: Bridge Gap Resonator
-- ===========================

ResonatorTube = {}
ResonatorTube.body = difference(
  cylinder(Resonator.outer_radius, Resonator.height),
  cylinder(Resonator.inner_radius, Resonator.height + 1),
  box(Resonator.gap, Resonator.wall * 2, Resonator.height):center(true, false, false):at(0, -Resonator.outer_radius)
):centered():material(copper)

ResonatorTube.gap_dielectric = box(Resonator.gap, Resonator.wall, Resonator.height)
    :center(true, false, true):at(0, -Resonator.outer_radius, 0):material(ptfe)

-- ===========================
-- Geometry: Bridge
-- ===========================

Bridge.body = difference(
  cylinder(Bridge.outer_radius, Resonator.height),
  cylinder(Resonator.outer_radius + Bridge.distance, Resonator.height),
  box(Bridge.outer_radius, Bridge.outer_radius * 2, Resonator.height):center(false, true, false):at(Bridge.width / 2, 0,
    0),
  box(Bridge.outer_radius, Bridge.outer_radius * 2, Resonator.height):center(false, true, false):at(
    -(Bridge.outer_radius + Bridge.width / 2), 0, 0),
  box(Bridge.outer_radius * 2, Bridge.outer_radius, Resonator.height):center(true, false, false)
):centered():material(copper)

Bridge.dielectric = difference(
  cylinder(Resonator.outer_radius + Bridge.distance, Resonator.height),
  cylinder(Resonator.outer_radius, Resonator.height),
  box(Bridge.outer_radius, Bridge.outer_radius * 2, Resonator.height):center(false, true, false):at(Bridge.width / 2, 0,
    0),
  box(Bridge.outer_radius, Bridge.outer_radius * 2, Resonator.height):center(false, true, false):at(
    -(Bridge.outer_radius + Bridge.width / 2), 0, 0),
  box(Bridge.outer_radius * 2, Bridge.outer_radius, Resonator.height):center(true, false, false)
):centered():material(ptfe)

-- ===========================
-- Geometry: Coupling Coil
-- ===========================

CouplingCoil.inner_radius = CouplingCoil.radius - CouplingCoil.wire_diameter / 2
CouplingCoil.outer_radius = CouplingCoil.radius + CouplingCoil.wire_diameter / 2

CouplingCoil.body = ring(CouplingCoil.inner_radius, CouplingCoil.outer_radius, CouplingCoil.wire_diameter)
    :at(0, 0, CouplingCoil.z_position - CouplingCoil.wire_diameter / 2)
    :material(copper)

-- ===========================
-- Geometry: Scaffold
-- ===========================

local tube_half_length = Scaffold.tube_length / 2

local scaffold_tube = difference(
  cylinder(Scaffold.tube_radius, Scaffold.tube_length):centered():rotate(0, 90, 0),
  cylinder(Scaffold.axial_hole_radius, Scaffold.tube_length + 2):centered():rotate(0, 90, 0)
)

local stopper_length = Coil.width + 2 * Scaffold.stopper_width
local coil_cutout_scale = 1.02

local coil_cutout_right = difference(
  cylinder(Coil.outer_radius, Coil.width):centered(),
  cylinder(Coil.inner_radius, Coil.width + 1):centered()
):scale(coil_cutout_scale):rotate(0, 90, 0):at(Coil.right_x, 0, 0)

local coil_cutout_left = difference(
  cylinder(Coil.outer_radius, Coil.width):centered(),
  cylinder(Coil.inner_radius, Coil.width + 1):centered()
):scale(coil_cutout_scale):rotate(0, 90, 0):at(Coil.left_x, 0, 0)

local stopper_right_base = difference(
  cylinder(Scaffold.stopper_radius, stopper_length):centered(),
  cylinder(Scaffold.tube_radius, stopper_length + 1):centered()
):rotate(0, 90, 0):at(Coil.right_x, 0, 0)

local stopper_right = difference(stopper_right_base, coil_cutout_right)

local stopper_left_base = difference(
  cylinder(Scaffold.stopper_radius, stopper_length):centered(),
  cylinder(Scaffold.tube_radius, stopper_length + 1):centered()
):rotate(0, 90, 0):at(Coil.left_x, 0, 0)

local stopper_left = difference(stopper_left_base, coil_cutout_left)

local resonator_box = box(Coil.gap, Scaffold.box_length, Scaffold.box_height * 2)
    :center(true, true, true)

local resonator_cutout = cylinder(Resonator.outer_radius + Scaffold.clearance, Scaffold.box_height * 2 + 2)
    :centered()

local bridge_cutout = box(
  Bridge.width + 2 * Scaffold.bridge_clearance,
  1.15 * Bridge.outer_radius + Scaffold.bridge_clearance,
  Scaffold.box_height * 2 + 2
):center(true, false, true)

Scaffold.body = difference(
  union(scaffold_tube, stopper_right, stopper_left, resonator_box),
  resonator_cutout,
  bridge_cutout,
  cylinder(Scaffold.axial_hole_radius, Scaffold.tube_length + 2):centered():rotate(0, 90, 0)
):color(0.15, 0.15, 0.15, 1.0):material(dark_pla)

-- ===========================
-- Assembly
-- ===========================

local assembly = group("helmholtz_coil", {
  RightCoil.body,
  LeftCoil.body,
  ResonatorTube.body,
  ResonatorTube.gap_dielectric,
  Bridge.body,
  Bridge.dielectric,
  Scaffold.body,
  CouplingCoil.body,
})

Mittens.register(assembly)

-- ===========================
-- Physics: Magnetostatic Study
-- ===========================

local ampere_turns = Coil.current * Coil.windings

local mag_study = magnetostatic({
  solver = "direct",
  formulation = "vector_potential",
  sources = {
    current_source(RightCoil.body, {
      current = Coil.current,
      turns = Coil.windings,
      direction = "ccw",
    }),
    current_source(LeftCoil.body, {
      current = Coil.current,
      turns = Coil.windings,
      direction = "ccw",
    }),
  },
}):domain(assembly):boundary("outer", {
  type = "magnetic_insulation",
  distance = Config.domain_radius,
}):mesh({
  type = "tetrahedral",
  max_element_size = 10,
  min_element_size = 1.0,
  curvature_factor = 0.25,
  refinement_regions = {
    { region = RightCoil.body, size = 2.0 },
    { region = LeftCoil.body,  size = 2.0 },
    { region = "sphere",       center = { 0, 0, 0 }, radius = Coil.gap * 0.4,          size = 1.5 },
    { region = "cylinder",     axis = "z",           radius = Coil.inner_radius * 0.8, z_min = -Coil.gap / 2, z_max = Coil.gap / 2, size = 3.0 },
  }
})

-- ===========================
-- Virtual Instruments
-- ===========================

local coil_center_x = Coil.gap / 2 + Coil.width / 2

GaussMeter({ 0, 0, 0 }, {
  range = "mT",
  component = "x",
  label = "B_center"
})

Probe("B_x_axis", {
  type = "B_field",
  line = { { -coil_center_x, 0, 0 }, { coil_center_x, 0, 0 } },
  points = 101,
  component = "x",
  export = "results/B_axial.csv"
})

Probe("B_z_axis", {
  type = "B_field",
  line = { { 0, 0, -Coil.inner_radius }, { 0, 0, Coil.inner_radius } },
  points = 51,
  component = "x",
  export = "results/B_radial.csv"
})

Probe("B_uniformity", {
  type = "B_field",
  volume = { center = { 0, 0, 0 }, radius = Coil.gap * 0.3 },
  samples = 1000,
  statistics = { "mean", "std", "min", "max" },
})

MagneticFieldPlane("XY", 0, {
  quantity = "B",
  style = "streamlines",
  scale = "linear",
  resolution = 50,
  streamline_density = 2.0,
  color_map = "viridis"
})

MagneticFieldPlane("XY_mag", 0, {
  quantity = "B_magnitude",
  style = "colormap",
  scale = "linear",
  resolution = 80,
  color_map = "plasma",
  range = "auto"
})

-- ===========================
-- Theoretical Calculations
-- ===========================

local mu0 = 4 * math.pi * 1e-7
local R = Coil.mean_radius * 1e-3
local d = Coil.center_distance * 1e-3

local helmholtz_factor = math.pow(4 / 5, 1.5)
local B_ideal = helmholtz_factor * mu0 * Coil.windings * Coil.current / R
local B_single_coil = mu0 * Coil.windings * Coil.current * R ^ 2 / (2 * math.pow(R ^ 2 + (d / 2) ^ 2, 1.5))
local B_total = 2 * B_single_coil

print("=== Helmholtz Coil Configuration ===")
print(string.format("Wire: %.2f mm diameter, packing factor %.2f", Wire.diameter, Wire.packing_factor))
print(string.format("Windings: %d total (%d layers x %d turns/layer)", Coil.layers * Coil.turns_per_layer, Coil.layers,
  Coil.turns_per_layer))
print(string.format("Coil cross-section: %.1f mm (axial) x %.1f mm (radial)", Coil.width, Coil.height))
print(string.format("Coil radii: inner=%.1f mm, mean=%.1f mm, outer=%.1f mm", Coil.inner_radius, Coil.mean_radius,
  Coil.outer_radius))
print(string.format("Gap between coils: %.1f mm", Coil.gap))
print(string.format("Center-to-center distance: %.1f mm", Coil.center_distance))
print(string.format("Helmholtz ratio (d/R): %.3f (ideal = 1.0)", Coil.center_distance / Coil.mean_radius))
print(string.format("Resonator tube: OD=%.1f mm, ID=%.1f mm, height=%.1f mm", Resonator.outer_radius * 2,
  Resonator.inner_radius * 2, Resonator.height))
print(string.format("Scaffold tube: OD=%.1f mm, length=%.1f mm", Scaffold.tube_radius * 2, Scaffold.tube_length))
print(string.format("Scaffold stoppers: OD=%.1f mm, width=%.1f mm", Scaffold.stopper_radius * 2, Scaffold.stopper_width))
print(string.format("Scaffold resonator box: %.1f x %.1f x %.1f mm (WxLxH)", Coil.gap, Scaffold.box_length,
  Scaffold.box_height * 2))
print(string.format("Scaffold bridge cutout: %.1f mm (clearance %.1f mm)",
  (Bridge.outer_radius + Scaffold.bridge_clearance) * 2, Scaffold.bridge_clearance))
print(string.format("Scaffold clearance: %.1f mm, axial hole: %.1f mm", Scaffold.clearance,
  Scaffold.axial_hole_radius * 2))
print("")
print("=== Theoretical Field Estimates ===")
print(string.format("Ampere-turns per coil: %.0f A·turns", ampere_turns))
print(string.format("Ideal Helmholtz B-field (d=R): %.3f mT", B_ideal * 1000))
print(string.format("Actual B-field estimate (d=%.1fmm): %.3f mT", Coil.center_distance, B_total * 1000))
print(string.format("Deviation from ideal: %.1f%%", (B_total / B_ideal - 1) * 100))

export_stl("helmholtz_scaffold.stl", Scaffold.body, 128)

-- ===========================
-- NanoVNA Frequency Sweep
-- ===========================

NanoVNA = {
  f_start = 1e6,
  f_stop = 100e6,
  num_points = 201,
  coil_radius = CouplingCoil.radius,
  num_turns = CouplingCoil.turns,
  wire_diameter = CouplingCoil.wire_diameter,
  coil_resistance = CouplingCoil.resistance,
}

view({
  flat_shading = true,
  circular_segments = 64,
})

return Mittens.serialize()
