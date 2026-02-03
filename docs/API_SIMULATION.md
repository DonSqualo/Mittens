# Simulation Module API Documentation

**Module:** `stdlib.simulation`  
**Purpose:** Infrastructure for fluid and flow simulation  
**Status:** Stage 11 - Ready for flow solver implementation

## Overview

The Simulation module provides data structures and configuration interfaces for time-stepped fluid simulations. It defines the core abstractions needed for flow solvers (Stokes, Navier-Stokes) to operate on channel networks with acoustic coupling.

**Key Components:**
- Time stepping and solver configuration
- Channel network representation (nodes, edges, connectivity)
- Boundary condition management
- Fluid material properties
- Simulation state and time advancement
- Integration hooks for visualization and output

## Core Classes

### TimeStepConfig

Configuration for time-stepping parameters.

```lua
local config = TimeStepConfig({
  dt = 0.001,              -- Time step (seconds)
  total_time = 1.0,        -- Total simulation time
  max_iterations = 1000,
  adaptive = false,        -- Adaptive timestep?
  max_dt = nil,
  min_dt = 1e-6,
  output_interval = 0.1,   -- Save state every N seconds
  convergence_tol = 1e-5,  -- For steady-state checks
})
```

**Properties:**
- `dt`: Time step size (default: 0.001 s)
- `total_time`: Total simulation duration (default: 1.0 s)
- `max_iterations`: Maximum number of steps
- `adaptive`: Enable adaptive timestep control (default: false)
- `convergence_tol`: Tolerance for steady-state detection

### SolverConfig

Specifies solver type and algorithm parameters.

```lua
local config = SolverConfig("stokes", {
  method = "direct",                -- direct, iterative, spectral
  preconditioner = "ilu",           -- For iterative solvers
  max_iterations = 100,
  tolerance = 1e-6,
  linearization = nil,              -- For nonlinear solvers
  time_integration = "backward_euler",  -- Scheme: explicit, implicit, etc.
  stokes_params = {},               -- Method-specific parameters
})
```

**Solver Types:**
- `"stokes"` - Linear Stokes flow (creeping flow, Re << 1)
- `"navier_stokes"` - Nonlinear Navier-Stokes flow
- `"acoustic"` - Acoustic wave propagation
- `"advection"` - Passive tracer transport

**Convenience Functions:**
```lua
local solver = Simulation.stokes_solver()
local solver = Simulation.navier_stokes_solver({
  method = "iterative",
  tolerance = 1e-6,
})
```

### ChannelNode

Represents a junction or boundary point in the channel network.

```lua
local node = ChannelNode("inlet_1", {
  position = {0, 0, 10},        -- {x, y, z} in mm
  node_type = "inlet",          -- inlet, outlet, junction, wall
  pressure = 101325,            -- Pressure BC value (Pa), optional
  flow_rate = 1e-6,            -- Flow rate BC value (m³/s), optional
  properties = {},              -- Custom properties
})
```

**Node Types:**
- `"inlet"` - Flow inlet (boundary)
- `"outlet"` - Flow outlet (boundary)
- `"junction"` - Internal connection point
- `"wall"` - Wall/no-slip boundary

### ChannelEdge

Represents a channel segment between two nodes.

```lua
local edge = ChannelEdge("main_trunk", {
  source_node = "inlet_1",
  target_node = "junction_1",
  diameter = 3.0,               -- mm (circular cross-section)
  length = 500,                 -- mm
  material = "water",           -- References FluidProperties
  roughness = 0.001,            -- Absolute roughness (mm)
  properties = {},
})
```

**Key Properties:**
- `diameter`: Channel cross-section diameter (mm)
- `length`: Channel length (mm)
- `material`: Fluid/material type (determines viscosity, etc.)
- `roughness`: Surface roughness for friction calculations

**Methods:**
```lua
local serialized = edge:serialize()
```

### ChannelNetwork

Container for nodes and edges with adjacency tracking.

```lua
local network = ChannelNetwork()

network:add_node(ChannelNode(...))
network:add_edge(ChannelEdge(...))

local node_count = network:node_count()
local edge_count = network:edge_count()
local neighbors = network:neighbors("node_id")
local node = network:get_node("node_id")
local edge = network:get_edge("edge_id")

local serialized = network:serialize()
```

**Methods:**
- `add_node(node)` - Add node to network
- `add_edge(edge)` - Add edge and update adjacency
- `get_node(id)` - Retrieve node by ID
- `get_edge(id)` - Retrieve edge by ID
- `neighbors(node_id)` - Get list of connected node IDs
- `node_count()` - Total number of nodes
- `edge_count()` - Total number of edges
- `serialize()` - Export to JSON-compatible format

### BoundaryCondition

Specifies boundary conditions on the domain.

```lua
local bc = BoundaryCondition("inlet", "pressure", {
  value = 101325,       -- BC value
  direction = {0, 0, 1},
  profile = "uniform",  -- uniform, parabolic, custom
  custom_fn = nil,      -- Custom profile function
})

-- Fluent API
bc:pressure(101325)
bc:flow_rate(1e-6)
bc:no_slip()
bc:slip()
```

**Condition Types:**
- `"pressure"` - Specify pressure at boundary
- `"flow_rate"` - Specify volumetric flow rate
- `"no_slip"` - Zero velocity at wall (no-slip)
- `"slip"` - Free-slip boundary condition
- `"inlet"` / `"outlet"` - Special boundary types

**Velocity Profiles:**
- `"uniform"` - Constant velocity
- `"parabolic"` - Poiseuille (pipe) profile
- `"custom"` - User-provided profile function

### FluidProperties

Material properties for a fluid.

```lua
local water = FluidProperties("water")  -- Uses defaults
local custom = FluidProperties("custom", {
  viscosity = 0.001,        -- Pa·s (dynamic viscosity)
  density = 1000,           -- kg/m³
  speed_of_sound = 1480,    -- m/s
  surface_tension = 0.072,  -- N/m (optional)
  bulk_modulus = 2.2e9,     -- Pa (optional)
})
```

**Built-in Fluids:**
- `"water"` - Pure water (1 mPa·s, 1000 kg/m³)
- `"glycerol"` - Glycerol (1.5 Pa·s, highly viscous)
- `"blood"` - Human blood (non-Newtonian, ~4 mPa·s)
- `"lymph"` - Lymphatic fluid (1.2 mPa·s, 1020 kg/m³)

**Methods:**
```lua
local nu = fluid:kinematic_viscosity()  -- Returns μ/ρ
local serialized = fluid:serialize()
```

### SimulationState

Tracks velocity, pressure, acoustic field, and other simulation data.

```lua
local state = SimulationState({
  channel_network = network,
  acoustic_field = field_data,
})

state:set_velocity("edge_id", velocity_value)
state:set_pressure("node_id", pressure_value)

local v = state:get_velocity("edge_id")
local p = state:get_pressure("node_id")

state:advance_time(0.001)  -- Increment time and iteration

local serialized = state:serialize()
```

**Properties:**
- `time` - Current simulation time (seconds)
- `iteration` - Current iteration number
- `velocity_field` - Dictionary mapping edge IDs to velocities
- `pressure_field` - Dictionary mapping node IDs to pressures
- `acoustic_field` - Reference to acoustic field (from Acoustics module)
- `force_field` - Body forces (e.g., from acoustic radiation)

**Methods:**
- `set_velocity(edge_id, value)` - Set velocity on edge
- `set_pressure(node_id, value)` - Set pressure at node
- `get_velocity(edge_id)` - Get current velocity
- `get_pressure(node_id)` - Get current pressure
- `advance_time(dt)` - Increment time by dt

### SimulationEngine

Main container for a complete simulation configuration.

```lua
local sim = SimulationEngine("lymph_flow", {
  timestep_config = TimeStepConfig(...),
  solver_config = SolverConfig(...),
  fluid_properties = FluidProperties("lymph"),
  channel_network = network,
})

-- Configure with fluent API
sim:with_timestep(...)
   :with_solver(...)
   :with_fluid(...)
   :with_network(...)

-- Boundary conditions
sim:add_boundary(BoundaryCondition(...))
local bc = sim:get_boundary("inlet_pressure")

-- Geometry integration
sim:link_geometry("body_layer", geometry_ref)
local geom = sim:get_geometry("body_layer")

-- Output hooks
sim:on_state_update(function(state)
  -- Called whenever state is updated
end)
sim:trigger_output()

-- State access
local state = sim:get_state()

-- Simulation stepping
sim:initialize()
sim:step()      -- Single timestep
sim:solve()     -- Run full simulation

local serialized = sim:serialize()
```

**Methods:**
- `with_timestep(config)` - Set timestep configuration
- `with_solver(config)` - Set solver configuration
- `with_fluid(props)` - Set fluid properties
- `with_network(network)` - Set channel network
- `add_boundary(bc)` - Add boundary condition
- `get_boundary(name)` - Retrieve boundary condition
- `boundaries()` - Get all boundary conditions
- `link_geometry(id, ref)` - Register geometry reference
- `get_geometry(id)` - Retrieve geometry reference
- `on_state_update(fn)` - Register output hook
- `trigger_output()` - Call all output hooks
- `get_state()` - Get current simulation state
- `initialize()` - Initialize simulation (placeholder)
- `step()` - Advance by one timestep
- `solve()` - Run complete simulation

## Convenience Functions

### Simulation.create(name, fluid_type)

Create a complete simulation with defaults.

```lua
local sim = Simulation.create("lymph_sim", "lymph")
-- Returns pre-configured SimulationEngine with:
--   - dt = 0.001 s
--   - total_time = 1.0 s
--   - Stokes solver with direct method
--   - Specified fluid properties
```

### Simulation.stokes_solver(config)

Create a Stokes flow solver (for creeping/viscous flow).

```lua
local solver = Simulation.stokes_solver({
  method = "direct",        -- Can override
  tolerance = 1e-8,         -- Tighter tolerance
  max_iterations = 200,
})
```

### Simulation.navier_stokes_solver(config)

Create a Navier-Stokes solver (for nonlinear flow).

```lua
local solver = Simulation.navier_stokes_solver({
  preconditioner = "ilu",
  tolerance = 1e-6,
  max_iterations = 300,
})
```

### Simulation.network_from_geometry(geometry, config)

Extract channel network from geometry (placeholder for future implementation).

```lua
local network = Simulation.network_from_geometry(body_geometry, {
  -- Configuration for network extraction
})
```

## Global Exports

The module is exported via `stdlib/init.lua` with convenient global shortcuts:

```lua
-- Classes
timestep_config = Simulation.TimeStepConfig
solver_config = Simulation.SolverConfig
channel_network = Simulation.ChannelNetwork
boundary_condition = Simulation.BoundaryCondition
fluid_properties = Simulation.FluidProperties
simulation = Simulation.SimulationEngine

-- Convenience functions
create_simulation = Simulation.create
```

## Usage Example

```lua
local Mittens = require("stdlib")

-- Create simulation
local sim = create_simulation("lymph_bath", "lymph")

-- Configure timestep
sim:with_timestep(timestep_config({
  dt = 0.001,
  total_time = 10.0,
}))

-- Configure solver
sim:with_solver(solver_config("stokes", {
  method = "direct",
  tolerance = 1e-8,
}))

-- Create channel network
local network = channel_network()
network:add_node(channel_node("inlet", {position = {-1000, 0, 200}}))
network:add_node(channel_node("outlet", {position = {1000, 0, 200}}))
network:add_edge(channel_edge("main", {
  source_node = "inlet",
  target_node = "outlet",
  length = 2000,
  diameter = 3.0,
}))

sim:with_network(network)

-- Add boundary conditions
sim:add_boundary(boundary_condition("inlet", "pressure", {value = 101325}))
sim:add_boundary(boundary_condition("outlet", "pressure", {value = 101000}))

-- Add output hook
sim:on_state_update(function(state)
  print(string.format("Time: %.3f s, Iteration: %d", state.time, state.iteration))
end)

-- Run
sim:solve()
```

## Design Notes

### Architecture

The module follows a **configuration-based** design:
- Data structures define domain (nodes, edges) and simulation setup
- Separate solver implementations consume these structures
- This allows multiple solver backends to coexist
- State is decoupled from solvers for flexibility

### API Philosophy

1. **Immutability by default** - Methods return `self` for chaining, mutations are explicit
2. **Fluent interfaces** - Configuration via method chains (e.g., `sim:with_fluid(...):with_solver(...)`)
3. **Serialization support** - All major structures have `:serialize()` methods
4. **Global shortcuts** - Common classes exported for convenience in scripts
5. **Explicit over implicit** - Configuration is verbose but clear

### Future Extensions

**Placeholder methods** (not yet implemented):
- `SimulationEngine:initialize()` - Initialize from geometry and ICs
- `Simulation.network_from_geometry()` - Extract network from solid model
- Actual solver implementations (will call `:step()` and `:solve()`)

These will be added in later stages when flow solvers are implemented.

## Error Handling

The module provides **structural validation** but relies on downstream solvers for **physics validation**.

- Node/edge references are not validated until solver time
- Boundary condition compatibility is not checked
- Network topology constraints are not enforced

**Future:** Add validation methods like `network:validate()` and `sim:check_setup()`.

## Performance

**Memory:**
- Grid-based simulations: O(nodes × edges)
- Typical bath network: ~50-100 nodes, ~100-200 edges
- Negligible memory for configuration objects

**Computation:**
- No computation at configuration time
- All calculations deferred to solvers
- Current module is purely declarative

## Integration Points

### With Acoustics Module

Simulation states include `acoustic_field` reference, allowing acoustic-flow coupling:

```lua
local sim = create_simulation("coupled", "water")
sim.state.acoustic_field = acoustic_field_data
-- Solver can then use acoustic pressure gradient as body force
```

### With Mittens Visualization

Output hooks enable frame-by-frame visualization:

```lua
sim:on_state_update(function(state)
  -- Export state to renderer
  _G.SimulationState = state:serialize()
end)
```

## Testing

Unit tests available in `test/test_simulation.lua`:
- Configuration object creation
- Network construction and adjacency
- Boundary condition management
- State advancement
- Serialization round-trip

Run: `lua test/test_simulation.lua`
