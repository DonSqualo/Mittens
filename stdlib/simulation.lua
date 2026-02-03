-- Mittens Standard Library: Fluid Simulation
-- Infrastructure and API for fluid/flow simulation
-- This module provides the core data structures and configuration interfaces
-- that stages 10-15 will build upon. Actual solver implementations come later.
--
-- Core Concepts:
--   - Simulations are time-stepped, configured with physics and geometry
--   - Solvers consume channel networks and produce flow/field data
--   - Boundary conditions drive the simulation
--   - Material properties define fluid behavior
--   - State updates hook into geometry and visualization

local Simulation = {}

-- =============================================================================
-- Time Stepping Configuration
-- =============================================================================

local function TimeStepConfig(config)
  config = config or {}
  
  local tsc = {
    _type = "timestep_config",
    dt = config.dt or 0.001,                    -- Time step (seconds)
    total_time = config.total_time or 1.0,      -- Total simulation time
    max_iterations = config.max_iterations or 1000,
    adaptive = config.adaptive or false,        -- Adaptive timestep?
    max_dt = config.max_dt,                     -- Max step for adaptive
    min_dt = config.min_dt or 1e-6,             -- Min step for adaptive
    output_interval = config.output_interval or 0.1,  -- When to save state
    convergence_tol = config.convergence_tol or 1e-5, -- For steady-state checks
  }
  
  return tsc
end

-- =============================================================================
-- Solver Configuration
-- =============================================================================

local function SolverConfig(solver_type, config)
  config = config or {}
  
  local solver = {
    _type = "solver_config",
    solver_type = solver_type,  -- "stokes", "navier_stokes", "acoustic", "advection"
    method = config.method or "direct",        -- direct, iterative, spectral, etc.
    preconditioner = config.preconditioner,    -- For iterative solvers
    max_iterations = config.max_iterations or 100,
    tolerance = config.tolerance or 1e-6,
    linearization = config.linearization,      -- For nonlinear solvers
    time_integration = config.time_integration or "backward_euler",  -- Scheme
    stokes_params = config.stokes_params or {},    -- Method-specific
  }
  
  setmetatable(solver, {__index = {
    stokes = function(self, params)
      self.stokes_params = params or {}
      return self
    end,
    
    navier_stokes = function(self, params)
      self.solver_type = "navier_stokes"
      self.stokes_params = params or {}
      return self
    end,
    
    serialize = function(self)
      return {
        type = "solver_config",
        solver_type = self.solver_type,
        method = self.method,
        preconditioner = self.preconditioner,
        max_iterations = self.max_iterations,
        tolerance = self.tolerance,
        time_integration = self.time_integration,
        params = self.stokes_params,
      }
    end
  }})
  
  return solver
end

-- =============================================================================
-- Channel Network Representation
-- =============================================================================

local function ChannelNode(id, config)
  config = config or {}
  
  local node = {
    _type = "channel_node",
    id = id,
    position = config.position or {0, 0, 0},  -- {x, y, z} in mm
    node_type = config.node_type or "junction",  -- junction, inlet, outlet, wall
    pressure = config.pressure,                 -- Pressure BC value (Pa)
    flow_rate = config.flow_rate,              -- Flow rate BC value (m³/s)
    properties = config.properties or {},       -- Custom properties
  }
  
  return node
end

local function ChannelEdge(id, config)
  config = config or {}
  
  local edge = {
    _type = "channel_edge",
    id = id,
    source_node = config.source_node,    -- ID of source node
    target_node = config.target_node,    -- ID of target node
    diameter = config.diameter or 1.0,   -- mm
    length = config.length or 10.0,      -- mm
    material = config.material or "water",
    roughness = config.roughness or 0.001,  -- Absolute roughness (mm)
    properties = config.properties or {},
  }
  
  setmetatable(edge, {__index = {
    serialize = function(self)
      return {
        id = self.id,
        source = self.source_node,
        target = self.target_node,
        diameter = self.diameter,
        length = self.length,
        material = self.material,
      }
    end
  }})
  
  return edge
end

local function ChannelNetwork(config)
  config = config or {}
  
  local network = {
    _type = "channel_network",
    nodes = {},           -- {node_id = ChannelNode, ...}
    edges = {},           -- {edge_id = ChannelEdge, ...}
    adjacency = {},       -- Connectivity: {node_id = {neighbor_ids...}}
    properties = config.properties or {},
  }
  
  setmetatable(network, {__index = {
    add_node = function(self, node)
      self.nodes[node.id] = node
      if not self.adjacency[node.id] then
        self.adjacency[node.id] = {}
      end
      return self
    end,
    
    add_edge = function(self, edge)
      self.edges[edge.id] = edge
      -- Update adjacency
      if not self.adjacency[edge.source_node] then
        self.adjacency[edge.source_node] = {}
      end
      if not self.adjacency[edge.target_node] then
        self.adjacency[edge.target_node] = {}
      end
      table.insert(self.adjacency[edge.source_node], edge.target_node)
      table.insert(self.adjacency[edge.target_node], edge.source_node)
      return self
    end,
    
    get_node = function(self, node_id)
      return self.nodes[node_id]
    end,
    
    get_edge = function(self, edge_id)
      return self.edges[edge_id]
    end,
    
    neighbors = function(self, node_id)
      return self.adjacency[node_id] or {}
    end,
    
    node_count = function(self)
      local count = 0
      for _ in pairs(self.nodes) do count = count + 1 end
      return count
    end,
    
    edge_count = function(self)
      local count = 0
      for _ in pairs(self.edges) do count = count + 1 end
      return count
    end,
    
    serialize = function(self)
      local nodes = {}
      for id, node in pairs(self.nodes) do
        nodes[id] = {
          position = node.position,
          type = node.node_type,
          pressure = node.pressure,
          flow_rate = node.flow_rate,
        }
      end
      
      local edges = {}
      for id, edge in pairs(self.edges) do
        edges[id] = edge:serialize()
      end
      
      return {
        type = "channel_network",
        nodes = nodes,
        edges = edges,
        adjacency = self.adjacency,
      }
    end
  }})
  
  return network
end

-- =============================================================================
-- Boundary Conditions
-- =============================================================================

local function BoundaryCondition(name, bc_type, config)
  config = config or {}
  
  local bc = {
    _type = "boundary_condition",
    name = name,
    condition_type = bc_type,  -- "pressure", "flow_rate", "no_slip", "slip", "inlet", "outlet"
    value = config.value,      -- Numeric value for pressure/flow_rate
    direction = config.direction or {0, 0, 1},  -- Direction vector
    profile = config.profile or "uniform",      -- "uniform", "parabolic", "custom"
    custom_fn = config.custom_fn,               -- For custom profiles
    properties = config.properties or {},
  }
  
  setmetatable(bc, {__index = {
    pressure = function(self, pa)
      self.condition_type = "pressure"
      self.value = pa
      return self
    end,
    
    flow_rate = function(self, m3_per_s)
      self.condition_type = "flow_rate"
      self.value = m3_per_s
      return self
    end,
    
    no_slip = function(self)
      self.condition_type = "no_slip"
      self.profile = "parabolic"
      return self
    end,
    
    slip = function(self)
      self.condition_type = "slip"
      return self
    end,
    
    serialize = function(self)
      return {
        type = "boundary_condition",
        name = self.name,
        condition_type = self.condition_type,
        value = self.value,
        profile = self.profile,
      }
    end
  }})
  
  return bc
end

-- =============================================================================
-- Fluid Material Properties
-- =============================================================================

local function FluidProperties(fluid_type, config)
  config = config or {}
  
  -- Default properties for common fluids
  local defaults = {
    water = {
      viscosity = 1e-3,           -- Pa·s at 20°C
      density = 1000,             -- kg/m³
      speed_of_sound = 1480,      -- m/s
      surface_tension = 0.072,    -- N/m
      bulk_modulus = 2.2e9,       -- Pa
    },
    glycerol = {
      viscosity = 1.5,            -- Pa·s at 20°C
      density = 1260,
      speed_of_sound = 1920,
      surface_tension = 0.064,
      bulk_modulus = 4.8e9,
    },
    blood = {
      viscosity = 4e-3,           -- Non-Newtonian, using nominal
      density = 1060,
      speed_of_sound = 1570,
      surface_tension = 0.06,
      bulk_modulus = 2.0e9,
      is_non_newtonian = true,
      shear_thinning_index = 0.8,  -- Power-law consistency
    },
    lymph = {
      viscosity = 1.2e-3,         -- Slightly higher than water
      density = 1020,
      speed_of_sound = 1490,
      surface_tension = 0.065,
      bulk_modulus = 2.1e9,
      protein_concentration = 0.02,  -- 20 g/L
    },
  }
  
  local props = defaults[fluid_type] or {}
  
  local fluid = {
    _type = "fluid_properties",
    fluid_type = fluid_type,
    viscosity = config.viscosity or props.viscosity or 1e-3,      -- Pa·s
    density = config.density or props.density or 1000,            -- kg/m³
    speed_of_sound = config.speed_of_sound or props.speed_of_sound or 1480,  -- m/s
    surface_tension = config.surface_tension or props.surface_tension,  -- N/m
    bulk_modulus = config.bulk_modulus or props.bulk_modulus,     -- Pa
    is_non_newtonian = config.is_non_newtonian or props.is_non_newtonian or false,
    shear_thinning_index = config.shear_thinning_index or props.shear_thinning_index or 1.0,
    properties = config.properties or {},
  }
  
  setmetatable(fluid, {__index = {
    kinematic_viscosity = function(self)
      return self.viscosity / self.density
    end,
    
    serialize = function(self)
      return {
        type = "fluid_properties",
        fluid_type = self.fluid_type,
        viscosity = self.viscosity,
        density = self.density,
        speed_of_sound = self.speed_of_sound,
        surface_tension = self.surface_tension,
        bulk_modulus = self.bulk_modulus,
        is_non_newtonian = self.is_non_newtonian,
      }
    end
  }})
  
  return fluid
end

-- =============================================================================
-- Simulation State
-- =============================================================================

local function SimulationState(config)
  config = config or {}
  
  local state = {
    _type = "simulation_state",
    time = 0,                    -- Current simulation time
    iteration = 0,               -- Current iteration count
    channel_network = config.channel_network,  -- ChannelNetwork reference
    velocity_field = {},         -- {edge_id = velocity}
    pressure_field = {},         -- {node_id = pressure}
    acoustic_field = config.acoustic_field or {},  -- Acoustic pressure field
    force_field = config.force_field or {},        -- Body forces
    visualization_state = {},    -- For rendering hooks
    properties = config.properties or {},
  }
  
  setmetatable(state, {__index = {
    set_velocity = function(self, edge_id, velocity)
      self.velocity_field[edge_id] = velocity
      return self
    end,
    
    set_pressure = function(self, node_id, pressure)
      self.pressure_field[node_id] = pressure
      return self
    end,
    
    get_velocity = function(self, edge_id)
      return self.velocity_field[edge_id] or 0
    end,
    
    get_pressure = function(self, node_id)
      return self.pressure_field[node_id] or 0
    end,
    
    advance_time = function(self, dt)
      self.time = self.time + dt
      self.iteration = self.iteration + 1
      return self
    end,
    
    serialize = function(self)
      return {
        type = "simulation_state",
        time = self.time,
        iteration = self.iteration,
        velocity_field = self.velocity_field,
        pressure_field = self.pressure_field,
        acoustic_field = self.acoustic_field,
      }
    end
  }})
  
  return state
end

-- =============================================================================
-- Simulation Engine (Main Configuration)
-- =============================================================================

local function SimulationEngine(name, config)
  config = config or {}
  
  local sim = {
    _type = "simulation",
    name = name,
    
    -- Core configuration
    timestep_config = config.timestep_config or TimeStepConfig(),
    solver_config = config.solver_config or SolverConfig("stokes"),
    fluid_properties = config.fluid_properties or FluidProperties("water"),
    
    -- Domain and network
    channel_network = config.channel_network or ChannelNetwork(),
    
    -- Boundary conditions
    boundary_conditions = config.boundary_conditions or {},
    
    -- State
    state = config.state or SimulationState(),
    
    -- Geometry coupling
    geometry_refs = {},  -- References to Mittens geometry for integration
    
    -- Visualization and output
    output_hooks = {},   -- Functions called on state updates
    visualization_config = config.visualization_config or {},
    
    -- Simulation metadata
    description = config.description or "",
    tags = config.tags or {},
  }
  
  setmetatable(sim, {__index = {
    -- Configuration builders (fluent API)
    with_timestep = function(self, tsc)
      self.timestep_config = tsc
      return self
    end,
    
    with_solver = function(self, solver)
      self.solver_config = solver
      return self
    end,
    
    with_fluid = function(self, fluid)
      self.fluid_properties = fluid
      return self
    end,
    
    with_network = function(self, network)
      self.channel_network = network
      return self
    end,
    
    -- Boundary condition management
    add_boundary = function(self, bc)
      self.boundary_conditions[bc.name] = bc
      return self
    end,
    
    get_boundary = function(self, name)
      return self.boundary_conditions[name]
    end,
    
    boundaries = function(self)
      local result = {}
      for name, bc in pairs(self.boundary_conditions) do
        table.insert(result, bc)
      end
      return result
    end,
    
    -- Geometry integration hooks
    link_geometry = function(self, geom_id, geom_ref)
      self.geometry_refs[geom_id] = geom_ref
      return self
    end,
    
    get_geometry = function(self, geom_id)
      return self.geometry_refs[geom_id]
    end,
    
    -- Output and visualization hooks
    on_state_update = function(self, hook_fn)
      table.insert(self.output_hooks, hook_fn)
      return self
    end,
    
    trigger_output = function(self)
      for _, hook in ipairs(self.output_hooks) do
        hook(self.state)
      end
      return self
    end,
    
    -- State access
    get_state = function(self)
      return self.state
    end,
    
    -- Initialization and stepping (placeholders for future solvers)
    initialize = function(self)
      -- Future: Initialize from geometry and ICs
      return self
    end,
    
    step = function(self)
      -- Future: Call actual solver to advance time
      local dt = self.timestep_config.dt
      self.state:advance_time(dt)
      self:trigger_output()
      return self
    end,
    
    -- ===================================================================
    -- FLOW SOLVER: 1D Poiseuille Flow with Acoustic Coupling
    -- ===================================================================
    
    --- Compute flow rates in channel network using Poiseuille equation
    -- Q = (π * r^4 * ΔP) / (8 * μ * L)
    -- where:
    --   r = radius (m)
    --   ΔP = pressure difference (Pa)
    --   μ = dynamic viscosity (Pa·s)
    --   L = channel length (m)
    solve_flow = function(self)
      local network = self.channel_network
      local state = self.state
      local fluid = self.fluid_properties
      
      if not network or network:edge_count() == 0 then
        return self
      end
      
      -- Iterate through each edge and compute flow rate using Poiseuille
      for edge_id, edge in pairs(network.edges) do
        -- Get source and target nodes
        local source_node = network:get_node(edge.source_node)
        local target_node = network:get_node(edge.target_node)
        
        if source_node and target_node then
          -- Get pressure at nodes (or use boundary conditions)
          local p_source = state:get_pressure(edge.source_node)
          local p_target = state:get_pressure(edge.target_node)
          
          -- Pressure difference (Pa)
          local delta_p = p_source - p_target
          
          -- Convert channel diameter from mm to m
          local radius = (edge.diameter / 2.0) / 1000.0
          
          -- Convert channel length from mm to m
          local length = edge.length / 1000.0
          
          -- Prevent division by zero
          if length < 1e-6 then
            length = 1e-6
          end
          
          -- Hagen-Poiseuille equation: Q = (π * r^4 * ΔP) / (8 * μ * L)
          local r4 = radius^4
          local mu = fluid.viscosity  -- Pa·s
          
          local flow_rate = (math.pi * r4 * delta_p) / (8.0 * mu * length)
          
          -- Store flow rate in state (positive = source to target)
          state:set_velocity(edge_id, flow_rate)
        end
      end
      
      return self
    end,
    
    --- Solve pressure field at all interior nodes using mass conservation
    -- At each junction node: sum of incoming flows = sum of outgoing flows
    -- This requires solving a linear system (Laplacian of pressure field)
    solve_pressure = function(self)
      local network = self.channel_network
      local state = self.state
      local fluid = self.fluid_properties
      
      if not network or network:node_count() == 0 then
        return self
      end
      
      -- Build linear system: A*p = b
      -- where p is the pressure vector at interior nodes
      local interior_nodes = {}
      local node_to_idx = {}
      local idx = 1
      
      -- Identify interior nodes (not on boundary)
      for node_id, node in pairs(network.nodes) do
        if node.node_type == "junction" then
          interior_nodes[idx] = node
          node_to_idx[node_id] = idx
          idx = idx + 1
        end
      end
      
      local n_interior = idx - 1
      
      if n_interior == 0 then
        return self  -- No interior nodes to solve for
      end
      
      -- Build system matrix and RHS
      -- For each edge, add conductance to the system
      local A = {}
      local b = {}
      
      for i = 1, n_interior do
        A[i] = {}
        for j = 1, n_interior do
          A[i][j] = 0
        end
        b[i] = 0
      end
      
      -- Process each edge
      for edge_id, edge in pairs(network.edges) do
        local source_node = network:get_node(edge.source_node)
        local target_node = network:get_node(edge.target_node)
        
        if source_node and target_node then
          -- Conductance: G = (π * r^4) / (8 * μ * L)
          local radius = (edge.diameter / 2.0) / 1000.0
          local length = edge.length / 1000.0
          if length < 1e-6 then length = 1e-6 end
          
          local r4 = radius^4
          local mu = fluid.viscosity
          local G = (math.pi * r4) / (8.0 * mu * length)
          
          -- Get node indices
          local i_src = node_to_idx[edge.source_node]
          local i_tgt = node_to_idx[edge.target_node]
          
          -- If source is boundary, add pressure contribution to RHS
          if not i_src then
            local p_src = source_node.pressure or 101325
            if i_tgt then
              b[i_tgt] = b[i_tgt] + G * p_src
            end
          else
            -- Interior source node
            A[i_src][i_src] = A[i_src][i_src] + G
          end
          
          -- If target is boundary, add pressure contribution to RHS
          if not i_tgt then
            local p_tgt = target_node.pressure or 101325
            if i_src then
              b[i_src] = b[i_src] + G * p_tgt
            end
          else
            -- Interior target node
            A[i_tgt][i_tgt] = A[i_tgt][i_tgt] + G
          end
          
          -- Add coupling between source and target (negative conductance)
          if i_src and i_tgt then
            A[i_src][i_tgt] = A[i_src][i_tgt] - G
            A[i_tgt][i_src] = A[i_tgt][i_src] - G
          end
        end
      end
      
      -- Simple Gaussian elimination solver for small systems
      -- (For production: use sparse solver, but this is for verification)
      local p_interior = self:_solve_linear_system(A, b)
      
      -- Store pressures in state
      if p_interior then
        for idx, node in ipairs(interior_nodes) do
          state:set_pressure(node.id, p_interior[idx])
        end
      end
      
      return self
    end,
    
    --- Add acoustic body forces to flow equations
    -- Acoustic radiation force: F = -∇p (from acoustic field)
    -- This creates an additional pressure gradient that drives flow
    couple_acoustic_force = function(self)
      local state = self.state
      local acoustic_field = state.acoustic_field
      
      if not acoustic_field then
        return self  -- No acoustic field
      end
      
      -- For each node, compute the acoustic pressure gradient
      -- This is stored as a body force that modifies the local pressure
      local network = self.channel_network
      
      for node_id, node in pairs(network.nodes) do
        if node.position and #node.position >= 3 then
          local x = node.position[1]  -- mm
          local z = node.position[3]  -- mm
          
          -- Get acoustic pressure at this location
          local p_acoustic = acoustic_field:pressure_at(x, z)
          
          -- Compute gradient (for body force calculation)
          local grad = acoustic_field:gradient(x, z)
          
          -- Store acoustic force magnitude at this node
          -- The force will be incorporated into the solver
          if not node.acoustic_force then
            node.acoustic_force = {}
          end
          
          node.acoustic_force.pressure = p_acoustic
          node.acoustic_force.gradient_x = grad.x or 0
          node.acoustic_force.gradient_z = grad.z or 0
        end
      end
      
      return self
    end,
    
    --- Complete flow solve: pressure + flow + acoustic coupling
    -- This is the main entry point for the flow solver
    step = function(self)
      -- Update acoustic field (if coupled)
      local dt = self.timestep_config.dt
      if self.state.acoustic_field then
        local acoustic_config = {
          bath = self.visualization_config.bath or {
            length = 2000,
            width = 600,
            depth = 400,
            center_z = 200,
          },
          sources = {},
          params = self.visualization_config.acoustic_params or {
            frequency = 0.02,
            amplitude = 100,
            phase_sweep = 0.1,
            medium_speed = 1524,
          }
        }
        
        -- If we have the acoustics module available, update the field
        local acoustics_available, Acoustics = pcall(require, "stdlib.acoustics")
        if acoustics_available and Acoustics.StandingWaveField then
          self.state.acoustic_field = Acoustics.StandingWaveField(
            acoustic_config.bath,
            acoustic_config.sources,
            acoustic_config.params,
            self.state.time
          )
        end
      end
      
      -- Solve flow in the network
      self:solve_pressure()      -- Solve pressure field from boundary conditions
      self:couple_acoustic_force() -- Add acoustic body forces
      self:solve_flow()          -- Compute flow rates using Poiseuille
      
      -- Advance time and trigger output
      self.state:advance_time(dt)
      self:trigger_output()
      
      return self
    end,
    
    --- Solve linear system A*x = b using Gaussian elimination
    -- For small systems (< 100 nodes), this is acceptable
    _solve_linear_system = function(self, A, b)
      local n = #b
      if n == 0 then return nil end
      
      -- Make copies to avoid modifying originals
      local A_copy = {}
      local b_copy = {}
      
      for i = 1, n do
        A_copy[i] = {}
        for j = 1, n do
          A_copy[i][j] = A[i][j] or 0
        end
        b_copy[i] = b[i] or 0
      end
      
      -- Forward elimination with partial pivoting
      for col = 1, n do
        -- Find pivot
        local max_row = col
        for row = col + 1, n do
          if math.abs(A_copy[row][col]) > math.abs(A_copy[max_row][col]) then
            max_row = row
          end
        end
        
        -- Swap rows
        A_copy[col], A_copy[max_row] = A_copy[max_row], A_copy[col]
        b_copy[col], b_copy[max_row] = b_copy[max_row], b_copy[col]
        
        -- Check for singular matrix
        if math.abs(A_copy[col][col]) < 1e-12 then
          return nil  -- Singular or ill-conditioned matrix
        end
        
        -- Eliminate column
        for row = col + 1, n do
          local factor = A_copy[row][col] / A_copy[col][col]
          for j = col, n do
            A_copy[row][j] = A_copy[row][j] - factor * A_copy[col][j]
          end
          b_copy[row] = b_copy[row] - factor * b_copy[col]
        end
      end
      
      -- Back substitution
      local x = {}
      for i = n, 1, -1 do
        x[i] = b_copy[i]
        for j = i + 1, n do
          x[i] = x[i] - A_copy[i][j] * x[j]
        end
        x[i] = x[i] / A_copy[i][i]
      end
      
      return x
    end,
    
    solve = function(self)
      -- Run full simulation with flow solving
      local dt = self.timestep_config.dt
      local total_time = self.timestep_config.total_time
      local iterations = math.floor(total_time / dt)
      
      self:initialize()
      for _ = 1, iterations do
        self:step()
      end
      return self
    end,
    
    -- Serialization
    serialize = function(self)
      return {
        type = "simulation",
        name = self.name,
        timestep_config = (self.timestep_config and type(self.timestep_config.serialize) == 'function') and self.timestep_config:serialize() or self.timestep_config,
        solver_config = (self.solver_config and type(self.solver_config.serialize) == 'function') and self.solver_config:serialize() or self.solver_config,
        fluid_properties = self.fluid_properties:serialize(),
        channel_network = self.channel_network:serialize(),
        boundary_conditions = self.boundary_conditions,
        state = self.state:serialize(),
      }
    end
  }})
  
  return sim
end

-- =============================================================================
-- Public API
-- =============================================================================

Simulation.TimeStepConfig = TimeStepConfig
Simulation.SolverConfig = SolverConfig
Simulation.ChannelNode = ChannelNode
Simulation.ChannelEdge = ChannelEdge
Simulation.ChannelNetwork = ChannelNetwork
Simulation.BoundaryCondition = BoundaryCondition
Simulation.FluidProperties = FluidProperties
Simulation.SimulationState = SimulationState
Simulation.SimulationEngine = SimulationEngine

-- =============================================================================
-- Convenience Functions
-- =============================================================================

--- Create a complete simulation with sensible defaults
-- @param name Simulation name
-- @param fluid_type Type of fluid (e.g., "water", "lymph", "blood")
-- @return Configured SimulationEngine
function Simulation.create(name, fluid_type)
  fluid_type = fluid_type or "water"
  local fluid = FluidProperties(fluid_type)
  local timestep = TimeStepConfig({
    dt = 0.001,
    total_time = 1.0,
  })
  local solver = SolverConfig("stokes", {
    method = "direct",
    tolerance = 1e-6,
  })
  
  return SimulationEngine(name, {
    timestep_config = timestep,
    solver_config = solver,
    fluid_properties = fluid,
  })
end

--- Create a flow solver optimized for Stokes flow
-- @param config Override config
-- @return Configured SolverConfig
function Simulation.stokes_solver(config)
  config = config or {}
  return SolverConfig("stokes", {
    method = config.method or "direct",
    tolerance = config.tolerance or 1e-8,
    max_iterations = config.max_iterations or 200,
    time_integration = "backward_euler",
  })
end

--- Create a flow solver optimized for Navier-Stokes
-- @param config Override config
-- @return Configured SolverConfig
function Simulation.navier_stokes_solver(config)
  config = config or {}
  return SolverConfig("navier_stokes", {
    method = config.method or "iterative",
    preconditioner = config.preconditioner or "ilu",
    tolerance = config.tolerance or 1e-6,
    max_iterations = config.max_iterations or 300,
    time_integration = "bdf2",
  })
end

--- Create a channel network from geometry
-- @param geometry Reference geometry object
-- @param config Network configuration
-- @return ChannelNetwork
function Simulation.network_from_geometry(geometry, config)
  config = config or {}
  local network = ChannelNetwork(config)
  
  -- Future: Extract nodes/edges from geometry topology
  -- For now, this is a placeholder for integration
  
  return network
end

-- Export convenience shortcuts
timestep_config = Simulation.TimeStepConfig
solver_config = Simulation.SolverConfig
channel_network = Simulation.ChannelNetwork
boundary_condition = Simulation.BoundaryCondition
fluid_properties = Simulation.FluidProperties
simulation = Simulation.SimulationEngine
create_simulation = Simulation.create

return Simulation
