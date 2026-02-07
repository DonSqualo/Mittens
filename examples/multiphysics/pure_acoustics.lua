-- pure_acoustics.lua
-- Ultrasound cell culture study setup
--
-- Components:
-- 1. Interchangeable Coverslip Dish (ICD) system
--    - Metal threaded base (stainless steel)
--    - Polycarbonate tube
--    - Silicone O-ring
--    - 30mm glass coverslip
-- 2. 96-well plate holder adapter
-- 3. Custom lid with ultrasound transducer holder

local Mittens = require("stdlib")

-- ============================================================================
-- Configuration Parameters
-- ============================================================================

Coverslip = {
  diameter = 30,
  thickness = 0.17, -- #1.5 thickness
  clear_aperture = 25,
}

Oring = {
  outer_diameter = 28,
  inner_diameter = 24,
  thickness = 0.2,
}

MetalBase = {
  outer_diameter = 35.5,
  inner_diameter = 30.5,
  height = 5,
  lip_height = 1.5,
  lip_inner_diameter = 26,
}

PolyTube = {
  outer_diameter = 32.5,
  inner_diameter = 26,
  height = 40,
}

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

HolderAdapter = {
  length = 128 + 1,
  width = 86 + 1,
  tolerance = 0.5,
  height = 10,
  offset = 1
}

Medium = {
  liquid_height = PolyTube.height - 2
}

Transducer = {
  diameter = 12,
  thickness = 2,
  height_from_coverslip = 35,
}

Lid = {
  diameter = 35,
  inner_diameter = 32,
  height = 5,
  wall_thickness = 1.5,
}
-- ============================================================================
-- Geometry: Metal Threaded Base (Stainless Steel)
-- Coverslip top face at z=0 (cell culture surface)
-- ============================================================================

local base_z = -Coverslip.thickness - MetalBase.lip_height

MetalBase.body = difference(
      cylinder(MetalBase.outer_diameter / 2, MetalBase.height),
      cylinder(MetalBase.inner_diameter / 2, MetalBase.height + 1)
    )
    :at(0, 0, -Coverslip.thickness)

MetalBase.lip = difference(
      cylinder(MetalBase.inner_diameter / 2, MetalBase.lip_height),
      cylinder(MetalBase.lip_inner_diameter / 2, MetalBase.lip_height + 1)
    )
    :at(0, 0, base_z)

MetalBase.model = group("metal_base", { MetalBase.body, MetalBase.lip })
    :material(material("steel"))

-- ============================================================================
-- Geometry: Coverslip (top face at z=0)
-- ============================================================================

local coverslip_z = -Coverslip.thickness
Coverslip.model = cylinder(Coverslip.diameter / 2, Coverslip.thickness)
    :at(0, 0, coverslip_z)
    :material(material("glass"))

-- ============================================================================
-- Geometry: O-ring (immediately at z=0, on top of coverslip)
-- ============================================================================

local oring_z = 0
Oring.model = difference(
      cylinder(Oring.outer_diameter / 2, Oring.thickness),
      cylinder(Oring.inner_diameter / 2, Oring.thickness)
    )
    :at(0, 0, oring_z)
    :material(material("rubber"))

-- ============================================================================
-- Geometry: Polycarbonate Tube (above o-ring)
-- ============================================================================

local tube_z = oring_z + Oring.thickness

PolyTube.model = difference(
      cylinder(PolyTube.outer_diameter / 2, PolyTube.height),
      cylinder(PolyTube.inner_diameter / 2, PolyTube.height + 1)
    )
    :at(0, 0, tube_z)
    :material(material("polycarbonate"))

-- ============================================================================
-- Assembly: Interchangeable Coverslip Dish (without medium, added later)
-- ============================================================================

local icd_assembly = group("icd_assembly", {
  Coverslip.model,
  Oring.model,
  MetalBase.model,
  PolyTube.model,
})

-- ============================================================================
-- Geometry: 96-Well Plate Holder Adapter
-- ============================================================================

Microscope.model = difference(
      box(Microscope.length, Microscope.width, Microscope.height):centerXY(),
      box(Microscope.WellPlate.length, Microscope.WellPlate.width, Microscope.height):centerXY():at(0, 0,
        Microscope.height - Microscope.WellPlate.offset),
      box(Microscope.WellPlate.length - Microscope.WellPlate.offset,
        Microscope.WellPlate.width - Microscope.WellPlate.offset,
        Microscope.height):centerXY()
    )
    :at(0, 0, -Microscope.height)
    :material(material("steel"))



HolderAdapter.model = difference(
      box(HolderAdapter.length, HolderAdapter.width, HolderAdapter.height):centerXY(),
      cylinder(MetalBase.outer_diameter / 2, HolderAdapter.height):at(0, 0, HolderAdapter.offset),
      cylinder(MetalBase.inner_diameter / 2, HolderAdapter.height)
    )
    :at(0, 0, -HolderAdapter.height)

-- ============================================================================
-- Geometry: Lid + Damper + Transducer Assembly
-- Transducer face at specific height from coverslip, damper connects to lid
-- ============================================================================

local lid_z = tube_z + PolyTube.height - (Lid.height - Lid.wall_thickness)
local lid_inner_ceiling_z = lid_z + (Lid.height - Lid.wall_thickness)
local transducer_z = Transducer.height_from_coverslip
local damper_z = transducer_z + Transducer.thickness

Damper = {
  diameter = Transducer.diameter,
  height = lid_inner_ceiling_z - damper_z,
}

Lid.outer = cylinder(Lid.diameter / 2, Lid.height)

Lid.inner = cylinder(
  Lid.inner_diameter / 2,
  Lid.height - Lid.wall_thickness
)

Lid.body = difference(Lid.outer, Lid.inner)
    :at(0, 0, lid_z)

Damper.model = cylinder(Damper.diameter / 2, Damper.height)
    :at(0, 0, damper_z)
    :material(material("petg"))

Transducer.model = cylinder(Transducer.diameter / 2, Transducer.thickness)
    :at(0, 0, transducer_z)
    :material(material("pzt"))

Lid.model = group("lid_assembly", {
  Lid.body,
  Damper.model,
  Transducer.model,
})

-- ============================================================================
-- Geometry: Culture Medium (water inside tube, displaced by transducer/damper)
-- ============================================================================

local displacement_height = lid_inner_ceiling_z - transducer_z
Medium.model = difference(
      cylinder(PolyTube.inner_diameter / 2 - 0.1, Medium.liquid_height),
      cylinder(Transducer.diameter / 2, displacement_height):at(0, 0, transducer_z)
    )
    :material(material("water"))

-- ============================================================================
-- Full Assembly
-- ============================================================================

local assembly = group("pure_acoustics", {
  icd_assembly,
  Medium.model,
  Microscope.model,
  HolderAdapter.model,
  Lid.model,
})

Mittens.register(assembly)

-- Export individual STL files for 3D printing (units: mm)
export_stl("icd.stl", icd_assembly)
export_stl("medium.stl", Medium.model)
export_stl("microscope.stl", Microscope.model)
export_stl("holder_adapter.stl", HolderAdapter.model)
export_stl("lid.stl", Lid.model)

-- ===========================
-- Acoustic Simulation
-- ===========================

Acoustic = {
  frequency = 1e6,
  drive_current = 0.1,
}

local acoustic_study = acoustic({
  frequency = Acoustic.frequency,
  drive_current = Acoustic.drive_current,
  transducer = Transducer.model,
  medium = Medium.model,
  boundaries = {
    acoustic_boundary(Coverslip.model, {
      type = "impedance",
      impedance = material("glass").acoustic_impedance,
    }),
    acoustic_boundary(PolyTube.model, {
      type = "impedance",
      impedance = material("polycarbonate").acoustic_impedance,
    }),
    acoustic_boundary(Damper.model, {
      type = "impedance",
      impedance = material("petg").acoustic_impedance,
    }),
  },
})

Mittens.register(acoustic_study)


-- ===========================
-- Circuit: Transducer Electronics
-- ===========================

local transducer_circuit = Circuit({
  components = {
    SignalGenerator({ frequency = Acoustic.frequency, amplitude = 1.0 }),
    Amplifier({ gain = 50 }),
    MatchingNetwork({
      impedance_real = 50,
      impedance_imag = -100,
      frequency = Acoustic.frequency,
    }),
    TransducerLoad({
      impedance_real = 50,
      impedance_imag = -100,
    }),
  },
  size = { 400, 90 },
})

Mittens.register(transducer_circuit)

-- ============================================================================
-- View Configuration
-- ============================================================================

view({
  camera = "isometric",
  distance = 150,
  target = { 0, 0, PolyTube.height / 2 },
  theme = "dark",
  axes = { show = true, size = 20 },
})

return Mittens.serialize()
