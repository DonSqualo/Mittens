-- lymph_bath.lua
-- Lymphatic drainage test bath with acoustic forcing
-- Reference: Hauglund et al. 2025 — 0.02 Hz vasomotion drives glymphatic clearance
--
-- Display: XZ plane (Y=0 slice), animated

local Mittens = require("stdlib")


-- Configuration


Bath = {
  length = 2000,
  width = 600,
  depth = 400,
  wall = 5,
}

Gel = {
  length = 1600,
  width = 400,
  height = 300,
  offset_z = 50,
}

-- Simplified channel network (2D, in XZ plane)
Channels = {
  diameter = 3,
  wall = 0.5,
  
  -- Main horizontal trunk
  trunk = {
    z = Gel.offset_z + Gel.height * 0.5,
    length = Gel.length * 0.9,
  },
  
  -- Secondary channels (vertical collectors)
  collectors = {
    count = 5,
    diameter = 1,
    spacing = Gel.length / 6,
    height = Gel.height * 0.3,
  },
}

-- Acoustic sources (speakers at bath ends)
Speakers = {
  diameter = 100,
  depth = 20,
  z = Bath.depth / 2,  -- centered vertically
}

-- Simulation parameters (for physics layer)
Simulation = {
  frequency = 0.02,       -- Hz (match natural vasomotion)
  amplitude = 100,        -- Pa
  phase_sweep = 0.1,      -- Hz (moving standing wave rate)
  medium_speed = 1524,    -- m/s (water at 37°C)
}


-- Materials


local aluminum = material("aluminum", {
  density = 2700,
  youngs_modulus = 70e9,
  poissons_ratio = 0.33,
})

local water = material("water_37C", {
  density = 993,
  speed_of_sound = 1524,
  viscosity = 0.00069,
})

local gel = material("tissue_gel", {
  density = 1040,
  speed_of_sound = 1540,
  storage_modulus = 2000,     -- Pa (G')
  loss_modulus = 200,         -- Pa (G'')
  attenuation = 0.5,          -- dB/cm/MHz
})

local lymph = material("lymph", {
  density = 1020,
  viscosity = 0.0018,
})


-- Geometry: Bath (outer shell)


local bath_outer = box(
  Bath.length + 2 * Bath.wall,
  Bath.width + 2 * Bath.wall,
  Bath.depth + Bath.wall
):center(true, true, false)

local bath_inner = box(Bath.length, Bath.width, Bath.depth)
  :center(true, true, false)
  :at(0, 0, Bath.wall)

local bath_shell = difference(bath_outer, bath_inner)
  :material(aluminum)
  :color(0.7, 0.7, 0.75, 1.0)
  :tag("bath_shell")

-- Water volume (for visualization) - very transparent to see inside
local water_volume = box(Bath.length, Bath.width, Bath.depth)
  :center(true, true, false)
  :at(0, 0, Bath.wall)
  :material(water)
  :color(0.2, 0.4, 0.8, 0.1)
  :tag("bath_water")


-- Geometry: Gel Block (tissue surrogate)


local gel_block = box(Gel.length, Gel.width, Gel.height)
  :center(true, true, false)
  :at(0, 0, Bath.wall + Gel.offset_z)
  :material(gel)
  :color(1.0, 0.5, 0.5, 0.8)
  :tag("gel_matrix")


-- Geometry: Channel Network (simplified lymphatic)


local channel_group = {}

-- Main trunk (horizontal, in XZ plane at Y=0)
local trunk_x_start = -Channels.trunk.length / 2
local trunk_x_end = Channels.trunk.length / 2
local trunk_z = Bath.wall + Channels.trunk.z

local trunk = cylinder(Channels.diameter / 2, Channels.trunk.length)
  :centered()
  :rotate(0, 90, 0)
  :at(0, 0, trunk_z)
  :material(lymph)
  :color(0.2, 1.0, 0.2, 1.0)
  :tag("channel_trunk")

table.insert(channel_group, trunk)

-- Collector channels (vertical, connecting to trunk)
local collector_start_x = trunk_x_start + Channels.collectors.spacing
for i = 1, Channels.collectors.count do
  local x = collector_start_x + (i - 1) * Channels.collectors.spacing
  local collector = cylinder(Channels.collectors.diameter / 2, Channels.collectors.height)
    :centered()
    :at(x, 0, trunk_z + Channels.collectors.height / 2)
    :material(lymph)
    :color(0.2, 1.0, 0.2, 1.0)
    :tag("channel_collector_" .. i)
  table.insert(channel_group, collector)
end

local channels = group("lymphatic_network", channel_group)


-- Geometry: Speakers (acoustic sources)


local speaker_left = cylinder(Speakers.diameter / 2, Speakers.depth)
  :centered()
  :rotate(0, 90, 0)
  :at(-Bath.length / 2 - Speakers.depth / 2, 0, Bath.wall + Speakers.z)
  :color(0.2, 0.2, 0.2, 1.0)
  :tag("speaker_left")

local speaker_right = cylinder(Speakers.diameter / 2, Speakers.depth)
  :centered()
  :rotate(0, 90, 0)
  :at(Bath.length / 2 + Speakers.depth / 2, 0, Bath.wall + Speakers.z)
  :color(0.2, 0.2, 0.2, 1.0)
  :tag("speaker_right")


-- Geometry: Structural Support Frame

-- Table-like frame supporting the bath from below
-- 4 vertical legs at corners, horizontal beams, cross-bracing for water load (~500kg)
-- Using 20x20mm aluminum T-slot extrusions

local Extrusions = require("stdlib.extrusions")

local frame = Extrusions.structural_frame(
  Bath.length + 200,
  Bath.width + 200,
  Bath.depth - 50,
  "20x20"
)
  :at(0, 0, -Bath.depth + 50)
  :material(aluminum)
  :color(0.7, 0.7, 0.75, 1.0)


-- Assembly


local assembly = group("lymph_bath", {
  bath_shell,
  water_volume,
  gel_block,
  channels,
  speaker_left,
  speaker_right,
  frame,
})

Mittens.register(assembly)


-- Simulation Setup (consumed by physics layer)


simulation({
  type = "acoustic_fluid",
  
  -- Acoustic field
  acoustic = {
    sources = {
      { tag = "speaker_left",  frequency = Simulation.frequency, amplitude = Simulation.amplitude, phase = 0 },
      { tag = "speaker_right", frequency = Simulation.frequency, amplitude = Simulation.amplitude, phase = math.pi },
    },
    medium = "bath_water",
    phase_sweep_rate = Simulation.phase_sweep,
  },
  
  -- Fluid network
  fluid = {
    network_tag = "lymphatic_network",
    boundary_conditions = {
      { tag = "channel_collector_1", type = "pressure", value = 0 },
      { tag = "channel_collector_5", type = "pressure", value = 0 },
    },
  },
  
  -- Time stepping
  time = {
    dt = 0.1,            -- seconds
    duration = 100,      -- seconds (2 full cycles at 0.02 Hz)
  },
  
  -- Output
  output = {
    fields = { "pressure", "velocity", "flow_rate" },
    plane = "XZ",
    plane_offset = 0,    -- Y = 0
  },
})


-- View Configuration


view({
  flat_shading = true,
  camera = {
    position = { 0, -5000, 500 },
    target = { 0, 0, 200 },
    up = { 0, 0, 1 },
  },
  -- XZ plane display (looking from Y toward XZ plane)
  projection = "perspective",
  fov = 30,
})


-- Stage 11: Acoustic Field Visualization

-- Generate standing wave acoustic field for visualization as XZ plane overlay

local Acoustics = require("stdlib.acoustics")

-- Create acoustic field using the default parameters from simulation config
local acoustic_field = Acoustics.create_default_field(
  Bath.length,
  Simulation.frequency,
  Simulation.amplitude
)

-- Register the field for the server to send to the renderer
_G.AcousticField = {
  _type = "acoustic_field",
  time = acoustic_field.time,
  frequency = acoustic_field.frequency,
  amplitude = acoustic_field.amplitude,
  wavelength = acoustic_field.wavelength,
  max_pressure = acoustic_field.max_pressure,
  grid_x_points = acoustic_field.grid_x_points,
  grid_z_points = acoustic_field.grid_z_points,
  x_min = acoustic_field.x_min,
  x_max = acoustic_field.x_max,
  z_min = acoustic_field.z_min,
  z_max = acoustic_field.z_max,
  -- Flatten the 2D pressure grid for transmission (1D array indexed by (x_idx-1)*grid_z + z_idx)
  pressure_data = (function()
    local flat = {}
    for x_idx = 1, acoustic_field.grid_x_points do
      for z_idx = 1, acoustic_field.grid_z_points do
        table.insert(flat, acoustic_field.pressure[x_idx][z_idx])
      end
    end
    return flat
  end)(),
}


-- Stage 14: Flow Visualization with Particles

-- Visualize flow through lymphatic channels using animated particles

local Particles = require("stdlib.particles")

-- Create channel network from geometry
local network = Mittens.simulation.ChannelNetwork({})

-- Add nodes for trunk and collectors
local trunk_center_z = Bath.wall + Channels.trunk.z
local trunk_x_start = -Channels.trunk.length / 2
local trunk_x_end = Channels.trunk.length / 2

-- Main trunk nodes
local trunk_inlet_node = Mittens.simulation.ChannelNode("trunk_inlet", {
  position = {trunk_x_start, 0, trunk_center_z},
  node_type = "inlet",
  flow_rate = 1e-8,  -- m^3/s (inlet)
})
network:add_node(trunk_inlet_node)

local trunk_outlet_node = Mittens.simulation.ChannelNode("trunk_outlet", {
  position = {trunk_x_end, 0, trunk_center_z},
  node_type = "outlet",
  pressure = 101325,  -- Pa
})
network:add_node(trunk_outlet_node)

-- Add main trunk edge
local trunk_length = math.sqrt(
  (trunk_x_end - trunk_x_start)^2 + 0 + 0
)
local trunk_edge = Mittens.simulation.ChannelEdge("trunk", {
  source_node = "trunk_inlet",
  target_node = "trunk_outlet",
  diameter = Channels.diameter,
  length = trunk_length,
  flow_rate = 1e-8,
})
network:add_edge(trunk_edge)

-- Add collector nodes and edges
local collector_start_x = trunk_x_start + Channels.collectors.spacing
local collector_z_top = trunk_center_z + Channels.collectors.height / 2
local collector_z_bot = trunk_center_z - Channels.collectors.height / 2

for i = 1, Channels.collectors.count do
  local x = collector_start_x + (i - 1) * Channels.collectors.spacing
  
  -- Collector inlet (at top)
  local inlet_node = Mittens.simulation.ChannelNode("collector_" .. i .. "_inlet", {
    position = {x, 0, collector_z_top},
    node_type = "inlet",
    flow_rate = 5e-9,
  })
  network:add_node(inlet_node)
  
  -- Collector junction (at trunk level)
  local junction_node = Mittens.simulation.ChannelNode("collector_" .. i .. "_junction", {
    position = {x, 0, trunk_center_z},
    node_type = "junction",
  })
  network:add_node(junction_node)
  
  -- Vertical collector edge (downward)
  local vertical_edge = Mittens.simulation.ChannelEdge("collector_" .. i, {
    source_node = "collector_" .. i .. "_inlet",
    target_node = "collector_" .. i .. "_junction",
    diameter = Channels.collectors.diameter,
    length = Channels.collectors.height,
    flow_rate = 5e-9,
  })
  network:add_edge(vertical_edge)
  
  -- Connect to trunk
  if i <= Channels.collectors.count / 2 then
    -- Left side collectors
    local to_trunk_edge = Mittens.simulation.ChannelEdge("collector_" .. i .. "_to_trunk", {
      source_node = "collector_" .. i .. "_junction",
      target_node = "trunk_outlet",
      diameter = Channels.collectors.diameter * 0.8,
      length = math.abs(trunk_x_end - x),
      flow_rate = 5e-9,
    })
    network:add_edge(to_trunk_edge)
  else
    -- Right side collectors connect back to trunk
    local to_trunk_edge = Mittens.simulation.ChannelEdge("collector_" .. i .. "_to_trunk", {
      source_node = "collector_" .. i .. "_junction",
      target_node = "trunk_outlet",
      diameter = Channels.collectors.diameter * 0.8,
      length = math.abs(trunk_x_end - x),
      flow_rate = 5e-9,
    })
    network:add_edge(to_trunk_edge)
  end
end

-- Create particle system
local particle_system = Particles.create({
  channel_network = network,
  particles_per_inlet = 2,
  max_particles = 50,
  spawn_interval = 0.2,
})

-- Initialize particles
particle_system:initialize()

-- Register the particle system for animation updates
_G.ParticleSystem = particle_system
_G.ChannelNetwork = network

-- Create initial flow visualization data export
local function serialize_particles()
  local particles_data = {}
  local max_flow = particle_system.max_flow_magnitude
  
  for i, particle in ipairs(particle_system.particles) do
    local color = particle:get_color(max_flow)
    particles_data[i] = {
      position = particle.position,
      color = color,
      flow_magnitude = particle.flow_magnitude,
    }
  end
  
  return particles_data
end

-- Export flow visualization data
_G.FlowVisualization = {
  _type = "flow_visualization",
  particle_count = particle_system.particle_count,
  max_flow_magnitude = particle_system.max_flow_magnitude,
  particles = serialize_particles(),
}


-- Debug Output


-- Debug: Count primitives in the scene
local function count_primitives(obj, name)
  if not obj then return 0 end
  if obj._type == "group" or obj._type == "assembly" then
    local count = 0
    for _, child in ipairs(obj._children or {}) do
      count = count + count_primitives(child, (obj._name or "?") .. "/")
    end
    print(string.format("  Group '%s': %d children -> %d primitives", obj._name or "?", #(obj._children or {}), count))
    return count
  elseif obj._type == "csg" then
    local count = 0
    for _, child in ipairs(obj._children or {}) do
      count = count + count_primitives(child, name .. "/csg")
    end
    return count
  elseif obj._type == "shape" then
    print(string.format("  Primitive: %s (%s)", obj._tag or obj._name or "?", obj._metadata and obj._metadata.primitive or "?"))
    return 1
  else
    return 1
  end
end

print("\n=== Scene Structure ===")
local total = count_primitives(assembly, "")
print(string.format("Total primitives in assembly: %d", total))

print("")
print("=== Lymph Bath Configuration ===")
print(string.format("Bath: %d x %d x %d mm", Bath.length, Bath.width, Bath.depth))
print(string.format("Gel block: %d x %d x %d mm", Gel.length, Gel.width, Gel.height))
print(string.format("Main channel: d=%.1f mm, length=%.0f mm", Channels.diameter, Channels.trunk.length))
print(string.format("Collectors: %d channels, d=%.1f mm", Channels.collectors.count, Channels.collectors.diameter))
print("")
print("=== Simulation Parameters ===")
print(string.format("Frequency: %.3f Hz (period=%.1f s)", Simulation.frequency, 1/Simulation.frequency))
print(string.format("Amplitude: %.0f Pa", Simulation.amplitude))
print(string.format("Phase sweep: %.2f Hz", Simulation.phase_sweep))
print(string.format("Wavelength in water: %.1f mm", Simulation.medium_speed / Simulation.frequency * 1000))
print("")
print("=== Acoustic Field ===")
print(string.format("Grid: %dx%d points", acoustic_field.grid_x_points, acoustic_field.grid_z_points))
print(string.format("Max pressure: %.1f Pa", acoustic_field.max_pressure))
print(string.format("Field registered for renderer: AcousticField"))
print("")
print("=== Flow Visualization (Particles) ===")
print(string.format("Particle system created with %d nodes, %d edges", network:node_count(), network:edge_count()))
print(string.format("Initial particles: %d", particle_system.particle_count))
print(string.format("Max flow magnitude: %.2e m³/s", particle_system.max_flow_magnitude))
print(string.format("Particle system registered for renderer: FlowVisualization"))

return Mittens.serialize()
