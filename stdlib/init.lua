-- Mittens Standard Library
-- Main entry point that loads all modules

local Mittens = {}

-- Load all modules
Mittens.primitives = require("stdlib.primitives")
Mittens.transforms = require("stdlib.transforms")
Mittens.materials = require("stdlib.materials")
Mittens.csg = require("stdlib.csg")
Mittens.groups = require("stdlib.groups")
Mittens.instruments = require("stdlib.instruments")
Mittens.physics = require("stdlib.physics")
Mittens.view = require("stdlib.view")
Mittens.export = require("stdlib.export")
Mittens.circuits = require("stdlib.circuits")
Mittens.extrusions = require("stdlib.extrusions")
Mittens.simulation = require("stdlib.simulation")
Mittens.acoustics = require("stdlib.acoustics")
Mittens.particles = require("stdlib.particles")

-- Export primitive functions to global scope
box = Mittens.primitives.box
cylinder = Mittens.primitives.cylinder
sphere = Mittens.primitives.sphere
torus = Mittens.primitives.torus
ring = Mittens.primitives.ring

-- Export transform functions
translate = Mittens.transforms.translate
rotate = Mittens.transforms.rotate
scale = Mittens.transforms.scale
mirror = Mittens.transforms.mirror
linear_pattern = Mittens.transforms.linear_pattern
circular_pattern = Mittens.transforms.circular_pattern
Vec3 = Mittens.transforms.Vec3
Mat4 = Mittens.transforms.Mat4

-- Export CSG functions
union = Mittens.csg.union
difference = Mittens.csg.difference
intersect = Mittens.csg.intersect

-- Export group functions
group = Mittens.groups.group
assembly = Mittens.groups.assembly
component = Mittens.groups.component

-- Export material functions
material = Mittens.materials.material

-- Export view functions
view = Mittens.view.view

-- Export physics functions
magnetostatic = Mittens.physics.magnetostatic
acoustic = Mittens.physics.acoustic
acoustic_source = Mittens.physics.acoustic_source
acoustic_boundary = Mittens.physics.acoustic_boundary
current_source = Mittens.physics.current_source
linspace = Mittens.physics.linspace
logspace = Mittens.physics.logspace

-- Export instrument functions
Probe = Mittens.instruments.Probe
GaussMeter = Mittens.instruments.GaussMeter
MagneticFieldPlane = Mittens.instruments.MagneticFieldPlane
AcousticPressurePlane = Mittens.instruments.AcousticPressurePlane
Hydrophone = Mittens.instruments.Hydrophone

-- Export file functions
export_stl = Mittens.export.export_stl
export_3mf = Mittens.export.export_3mf

-- Export circuit functions
SignalGenerator = Mittens.circuits.SignalGenerator
Amplifier = Mittens.circuits.Amplifier
MatchingNetwork = Mittens.circuits.MatchingNetwork
TransducerLoad = Mittens.circuits.TransducerLoad
Circuit = Mittens.circuits.Circuit

-- Export extrusion functions
extrusion = Mittens.extrusions.profile
corner_bracket = Mittens.extrusions.corner_bracket
structural_frame = Mittens.extrusions.structural_frame

-- Export simulation functions
timestep_config = Mittens.simulation.TimeStepConfig
solver_config = Mittens.simulation.SolverConfig
channel_network = Mittens.simulation.ChannelNetwork
boundary_condition = Mittens.simulation.BoundaryCondition
fluid_properties = Mittens.simulation.FluidProperties
create_simulation = Mittens.simulation.create

-- Export acoustics functions
acoustic_source = Mittens.acoustics.AcousticSource
standing_wave_field = Mittens.acoustics.StandingWaveField
acoustic_simulation = Mittens.acoustics.AcousticSimulation
pressure_to_color = Mittens.acoustics.PressureToColor

-- Simulation config storage (stub - physics not implemented yet)
Mittens._simulation_config = nil

--- Register a simulation configuration (stub)
-- Captures config for future physics layer implementation
-- @param config Simulation configuration table
function simulation(config)
  Mittens._simulation_config = config
  if config then
    print("[simulation] Config captured (physics layer not yet implemented)")
    print(string.format("  Type: %s", config.type or "unknown"))
  end
end

-- Scene registry
Mittens._scene = {
  objects = {},
  instruments = {},
  studies = {},
  exports = {},
}

--- Register an object in the scene
-- @param obj Object to register
function Mittens.register(obj)
  if obj._type == "instrument" then
    table.insert(Mittens._scene.instruments, obj)
  elseif obj._type == "study" then
    table.insert(Mittens._scene.studies, obj)
  elseif obj._type == "visualization" then
    table.insert(Mittens._scene.instruments, obj)
  else
    table.insert(Mittens._scene.objects, obj)
  end
end

--- Get the full scene for rendering
-- @return Scene data
function Mittens.get_scene()
  return {
    objects = Mittens._scene.objects,
    instruments = Mittens.instruments.serialize_all(),
    studies = Mittens.physics.get_studies(),
    exports = Mittens.export.serialize(),
    view = Mittens.view.serialize(),
  }
end

--- Serialize the entire scene to JSON-compatible format
-- @return Serialized scene
function Mittens.serialize()
  local scene = Mittens.get_scene()

  -- Serialize objects
  local objects_serialized = {}
  for i, obj in ipairs(scene.objects) do
    if obj.serialize then
      objects_serialized[i] = obj:serialize()
    end
  end

  -- Serialize studies
  local studies_serialized = {}
  for i, study in ipairs(scene.studies) do
    if study.serialize then
      studies_serialized[i] = study:serialize()
    end
  end

  return {
    objects = objects_serialized,
    instruments = scene.instruments,
    studies = studies_serialized,
    exports = scene.exports,
    view = scene.view,
  }
end

--- Clear the scene
function Mittens.clear()
  Mittens._scene = {
    objects = {},
    instruments = {},
    studies = {},
    exports = {},
  }
  Mittens.instruments.clear()
  Mittens.physics.clear()
  Mittens.export.clear()
  Mittens.view.reset()
end

-- Version info
Mittens.VERSION = "0.1.0"
Mittens.NAME = "Mittens"

return Mittens
