-- helmholtz_fdtd_demo.lua
-- Comprehensive Helmholtz coil demo integrating:
-- - Helmholtz coils geometry with bridge-gap resonator
-- - FdtdStudy{} for EM simulation (oscilloscope)
-- - NanoVNA sweep for S11 reflection coefficient
-- - MagneticFieldPlane for 2D B-field visualization
-- - GaussMeter probe at center
-- - Circuit diagram for drive electronics

local Mittens = require("stdlib")

-- ============================================================================
-- Configuration Parameters
-- ============================================================================

Config = {
  domain_radius = 100,
  domain_height = 120,
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
  height = 60,
  gap = 0.8
}
Resonator.outer_radius = Coil.gap / 2
Resonator.inner_radius = Resonator.outer_radius - Resonator.wall

CouplingCoil = {
  turns = 1,
  radius = Resonator.outer_radius,
  wire_diameter = 1.5,
  distance = 5.0,
  resistance = 0.1,
}
CouplingCoil.z_position = Resonator.height / 2 + CouplingCoil.distance

-- ============================================================================
-- Materials
-- ============================================================================

local copper = material("copper", {
  conductivity = 5.8e7,
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

-- ============================================================================
-- Geometry: Right Coil
-- ============================================================================

local RightCoil = difference(
  cylinder(Coil.outer_radius, Coil.width),
  cylinder(Coil.inner_radius, Coil.width + 1)
):centered():rotate(0, 90, 0):at(Coil.right_x, 0, 0):material(copper)

-- ============================================================================
-- Geometry: Left Coil
-- ============================================================================

local LeftCoil = difference(
  cylinder(Coil.outer_radius, Coil.width),
  cylinder(Coil.inner_radius, Coil.width + 1)
):centered():rotate(0, 90, 0):at(Coil.left_x, 0, 0):material(copper)

-- ============================================================================
-- Geometry: Bridge Gap Resonator (BLGR)
-- ============================================================================

local ResonatorBody = difference(
  cylinder(Resonator.outer_radius, Resonator.height),
  cylinder(Resonator.inner_radius, Resonator.height + 1),
  box(Resonator.gap, Resonator.wall * 2, Resonator.height):center(true, false, false):at(0, -Resonator.outer_radius)
):centered():material(copper)

local ResonatorDielectric = box(Resonator.gap, Resonator.wall, Resonator.height)
    :center(true, false, true):at(0, -Resonator.outer_radius, 0):material(ptfe)

-- ============================================================================
-- Geometry: Coupling Coil (for NanoVNA measurement)
-- ============================================================================

CouplingCoil.inner_radius = CouplingCoil.radius - CouplingCoil.wire_diameter / 2
CouplingCoil.outer_radius = CouplingCoil.radius + CouplingCoil.wire_diameter / 2

local CouplingCoilBody = ring(CouplingCoil.inner_radius, CouplingCoil.outer_radius, CouplingCoil.wire_diameter)
    :at(0, 0, CouplingCoil.z_position - CouplingCoil.wire_diameter / 2)
    :material(copper)

-- ============================================================================
-- Assembly
-- ============================================================================

local assembly = group("helmholtz_fdtd_demo", {
  RightCoil,
  LeftCoil,
  ResonatorBody,
  ResonatorDielectric,
  CouplingCoilBody,
})

Mittens.register(assembly)

-- ============================================================================
-- FDTD Study Configuration
-- Triggers internal Rust FDTD solver for EM simulation
-- Results displayed on oscilloscope widget
-- ============================================================================

FdtdStudy = {
  -- Center frequency targeting resonator
  freq_center = 30e6,     -- 30 MHz center
  freq_width = 50e6,      -- 50 MHz bandwidth for broadband excitation

  -- Grid resolution (mm) - larger cells to keep memory reasonable
  cell_size = 4.0,        -- 4mm cells (coarser for reasonable memory)

  -- PML absorbing boundary layers
  pml_thickness = 4,      -- 4 cells of PML

  -- Simulation time (ns)
  max_time_ns = 50.0,     -- 50 ns maximum simulation

  -- Source position (inside resonator gap)
  source_offset = {0, -15, 0},

  -- Monitor position (at center)
  monitor_offset = {0, 0, 0},

  -- Field plane for visualization
  field_plane = "XY",     -- XY slice through center
}

-- ============================================================================
-- NanoVNA Frequency Sweep Configuration
-- Simulates S11 measurement from coupling coil
-- ============================================================================

NanoVNA = {
  f_start = 1e6,
  f_stop = 100e6,
  num_points = 201,
  coil_radius = CouplingCoil.radius,
  num_turns = CouplingCoil.turns,
  wire_diameter = CouplingCoil.wire_diameter,
  coil_resistance = CouplingCoil.resistance,
  -- Enable coupled resonator model for BLGR
  resonator_radius = Resonator.outer_radius,
  resonator_distance = CouplingCoil.distance,
  resonator_resistance = 0.05,  -- Low resistance for copper resonator
}

-- ============================================================================
-- Virtual Instruments
-- ============================================================================

-- GaussMeter at center to measure B-field
GaussMeter({0, 0, 0}, {
  range = "mT",
  component = "x",
  label = "B_center"
})

-- Additional probes along the axis
GaussMeter({Coil.gap/4, 0, 0}, {
  range = "mT",
  component = "x",
  label = "B_quarter"
})

-- MagneticFieldPlane for 2D B-field visualization
MagneticFieldPlane("XY", 0, {
  quantity = "B",
  style = "streamlines",
  scale = "linear",
  resolution = 50,
  streamline_density = 2.0,
  color_map = "viridis"
})

-- ============================================================================
-- Circuit: Drive Electronics
-- ============================================================================

local drive_circuit = Circuit({
  components = {
    SignalGenerator({ frequency = FdtdStudy.freq_center, amplitude = 1.0 }),
    Amplifier({ gain = 20 }),
    MatchingNetwork({
      impedance_real = 50,
      impedance_imag = -80,
      frequency = FdtdStudy.freq_center,
      use_nanovna = true,  -- Pull impedance from NanoVNA sweep
    }),
    TransducerLoad({
      impedance_real = 50,
      impedance_imag = -80,
    }),
  },
  size = { 400, 90 },
})

Mittens.register(drive_circuit)

-- ============================================================================
-- Theoretical Calculations (for console output)
-- ============================================================================

local mu0 = 4 * math.pi * 1e-7
local R = Coil.mean_radius * 1e-3
local d = Coil.center_distance * 1e-3
local ampere_turns = Coil.current * Coil.windings

local helmholtz_factor = math.pow(4 / 5, 1.5)
local B_ideal = helmholtz_factor * mu0 * Coil.windings * Coil.current / R
local B_single_coil = mu0 * Coil.windings * Coil.current * R ^ 2 / (2 * math.pow(R ^ 2 + (d / 2) ^ 2, 1.5))
local B_total = 2 * B_single_coil

print("=== Helmholtz FDTD Demo Configuration ===")
print(string.format("Wire: %.2f mm diameter, packing factor %.2f", Wire.diameter, Wire.packing_factor))
print(string.format("Windings: %d total (%d layers x %d turns/layer)", Coil.layers * Coil.turns_per_layer, Coil.layers, Coil.turns_per_layer))
print(string.format("Coil radii: inner=%.1f mm, mean=%.1f mm, outer=%.1f mm", Coil.inner_radius, Coil.mean_radius, Coil.outer_radius))
print(string.format("Gap between coils: %.1f mm", Coil.gap))
print(string.format("Helmholtz ratio (d/R): %.3f (ideal = 1.0)", Coil.center_distance / Coil.mean_radius))
print("")
print("=== Theoretical Field Estimates ===")
print(string.format("Ampere-turns per coil: %.0f A·turns", ampere_turns))
print(string.format("Ideal Helmholtz B-field (d=R): %.3f mT", B_ideal * 1000))
print(string.format("Actual B-field estimate: %.3f mT", B_total * 1000))
print("")
print("=== FDTD Study Parameters ===")
print(string.format("Center frequency: %.1f MHz", FdtdStudy.freq_center / 1e6))
print(string.format("Bandwidth: %.1f MHz", FdtdStudy.freq_width / 1e6))
print(string.format("Cell size: %.1f mm", FdtdStudy.cell_size))
print(string.format("Simulation time: %.1f ns", FdtdStudy.max_time_ns))
print("")
print("=== NanoVNA Sweep Parameters ===")
print(string.format("Frequency range: %.1f - %.1f MHz", NanoVNA.f_start / 1e6, NanoVNA.f_stop / 1e6))
print(string.format("Points: %d", NanoVNA.num_points))
print(string.format("Coupling coil: R=%.1f mm, N=%d turns", NanoVNA.coil_radius, NanoVNA.num_turns))

-- ============================================================================
-- View Configuration
-- ============================================================================

view({
  camera = {
    position = {100, -120, 80},
    target = {0, 0, 0},
    fov = 45
  },
  flat_shading = true,
  circular_segments = 64,
})

-- ============================================================================
-- Return scene for renderer
-- ============================================================================

return Mittens.serialize()
