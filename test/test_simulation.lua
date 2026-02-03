-- test_simulation.lua
-- Unit tests for stdlib/simulation.lua simulation infrastructure
--
-- Tests cover:
--   - Configuration objects
--   - Channel network construction
--   - Boundary condition management
--   - Fluid properties
--   - Simulation state and engine

local Simulation = require("stdlib.simulation")
local Test = {}

-- =============================================================================
-- Helper Functions
-- =============================================================================

local function assert_equal(actual, expected, msg)
  msg = msg or "Values not equal"
  if actual ~= expected then
    error(string.format("%s: expected %s, got %s", msg, tostring(expected), tostring(actual)))
  end
end

local function assert_true(value, msg)
  msg = msg or "Expected true"
  if not value then error(msg) end
end

local function assert_nil(value, msg)
  msg = msg or "Expected nil"
  if value ~= nil then error(msg) end
end

local function assert_near(actual, expected, tolerance, msg)
  tolerance = tolerance or 1e-6
  msg = msg or "Values not nearly equal"
  if math.abs(actual - expected) > tolerance then
    error(string.format("%s: expected %.6f, got %.6f", msg, expected, actual))
  end
end

-- =============================================================================
-- Test 1: TimeStepConfig Creation
-- =============================================================================

function Test.test_timestep_config()
  local tsc = Simulation.TimeStepConfig({
    dt = 0.001,
    total_time = 1.0,
    max_iterations = 1000,
  })
  
  assert_equal(tsc._type, "timestep_config", "Type should be timestep_config")
  assert_equal(tsc.dt, 0.001, "dt should be 0.001")
  assert_equal(tsc.total_time, 1.0, "total_time should be 1.0")
  assert_equal(tsc.max_iterations, 1000, "max_iterations should be 1000")
  assert_equal(tsc.adaptive, false, "adaptive should default to false")
  
  print("✓ Test 1 PASSED: TimeStepConfig creation")
end

-- =============================================================================
-- Test 2: SolverConfig Creation
-- =============================================================================

function Test.test_solver_config()
  local solver = Simulation.SolverConfig("stokes", {
    method = "direct",
    tolerance = 1e-6,
  })
  
  assert_equal(solver._type, "solver_config", "Type should be solver_config")
  assert_equal(solver.solver_type, "stokes", "Solver type should be stokes")
  assert_equal(solver.method, "direct", "Method should be direct")
  assert_equal(solver.tolerance, 1e-6, "Tolerance should be 1e-6")
  
  print("✓ Test 2 PASSED: SolverConfig creation")
end

-- =============================================================================
-- Test 3: Fluid Properties - Water
-- =============================================================================

function Test.test_fluid_properties_water()
  local fluid = Simulation.FluidProperties("water")
  
  assert_equal(fluid._type, "fluid_properties", "Type should be fluid_properties")
  assert_equal(fluid.fluid_type, "water", "Fluid type should be water")
  assert_near(fluid.viscosity, 1e-3, 1e-4, "Water viscosity at 20°C should be ~1e-3")
  assert_equal(fluid.density, 1000, "Water density should be 1000 kg/m³")
  assert_near(fluid.speed_of_sound, 1480, 10, "Water speed of sound should be ~1480 m/s")
  
  print("✓ Test 3 PASSED: Fluid properties (water)")
end

-- =============================================================================
-- Test 4: Fluid Properties - Lymph
-- =============================================================================

function Test.test_fluid_properties_lymph()
  local fluid = Simulation.FluidProperties("lymph")
  
  assert_equal(fluid.fluid_type, "lymph", "Fluid type should be lymph")
  assert_near(fluid.viscosity, 1.2e-3, 1e-4, "Lymph viscosity should be ~1.2e-3")
  assert_equal(fluid.density, 1020, "Lymph density should be 1020 kg/m³")
  assert_true(fluid.protein_concentration ~= nil, "Lymph should have protein concentration")
  
  print("✓ Test 4 PASSED: Fluid properties (lymph)")
end

-- =============================================================================
-- Test 5: Fluid Properties - Custom Override
-- =============================================================================

function Test.test_fluid_properties_custom()
  local fluid = Simulation.FluidProperties("custom", {
    viscosity = 0.5,
    density = 800,
    speed_of_sound = 1400,
  })
  
  assert_equal(fluid.viscosity, 0.5, "Custom viscosity should be respected")
  assert_equal(fluid.density, 800, "Custom density should be respected")
  assert_equal(fluid.speed_of_sound, 1400, "Custom speed_of_sound should be respected")
  
  print("✓ Test 5 PASSED: Fluid properties (custom override)")
end

-- =============================================================================
-- Test 6: Kinematic Viscosity Calculation
-- =============================================================================

function Test.test_kinematic_viscosity()
  local fluid = Simulation.FluidProperties("water")
  
  local nu = fluid:kinematic_viscosity()
  local expected = 1e-3 / 1000  -- μ/ρ = 1e-3 / 1000 = 1e-6
  
  assert_near(nu, expected, 1e-7, "Kinematic viscosity calculation wrong")
  
  print("✓ Test 6 PASSED: Kinematic viscosity calculation")
end

-- =============================================================================
-- Test 7: Channel Node Creation
-- =============================================================================

function Test.test_channel_node()
  local node = Simulation.ChannelNode("inlet_1", {
    position = {0, 0, 10},
    node_type = "inlet",
    pressure = 101325,
  })
  
  assert_equal(node._type, "channel_node", "Type should be channel_node")
  assert_equal(node.id, "inlet_1", "ID should be inlet_1")
  assert_equal(node.node_type, "inlet", "Node type should be inlet")
  assert_equal(node.pressure, 101325, "Pressure should be 101325")
  
  print("✓ Test 7 PASSED: Channel node creation")
end

-- =============================================================================
-- Test 8: Channel Edge Creation
-- =============================================================================

function Test.test_channel_edge()
  local edge = Simulation.ChannelEdge("main_trunk", {
    source_node = "inlet_1",
    target_node = "junction_1",
    diameter = 3.0,
    length = 500,
    material = "water",
  })
  
  assert_equal(edge._type, "channel_edge", "Type should be channel_edge")
  assert_equal(edge.id, "main_trunk", "ID should be main_trunk")
  assert_equal(edge.source_node, "inlet_1", "Source should be inlet_1")
  assert_equal(edge.target_node, "junction_1", "Target should be junction_1")
  assert_equal(edge.diameter, 3.0, "Diameter should be 3.0 mm")
  assert_equal(edge.length, 500, "Length should be 500 mm")
  
  print("✓ Test 8 PASSED: Channel edge creation")
end

-- =============================================================================
-- Test 9: Channel Network Construction
-- =============================================================================

function Test.test_channel_network()
  local network = Simulation.ChannelNetwork()
  
  -- Add nodes
  local node1 = Simulation.ChannelNode("n1", {position = {0, 0, 0}})
  local node2 = Simulation.ChannelNode("n2", {position = {100, 0, 0}})
  
  network:add_node(node1)
  network:add_node(node2)
  
  assert_equal(network:node_count(), 2, "Should have 2 nodes")
  
  -- Add edge
  local edge = Simulation.ChannelEdge("e1", {
    source_node = "n1",
    target_node = "n2",
    length = 100,
  })
  
  network:add_edge(edge)
  
  assert_equal(network:edge_count(), 1, "Should have 1 edge")
  
  -- Test adjacency
  local neighbors = network:neighbors("n1")
  assert_true(neighbors ~= nil, "Should have neighbors list")
  
  print("✓ Test 9 PASSED: Channel network construction")
end

-- =============================================================================
-- Test 10: Boundary Conditions
-- =============================================================================

function Test.test_boundary_conditions()
  local bc_pressure = Simulation.BoundaryCondition("inlet", "pressure", {
    value = 101325,
  })
  
  assert_equal(bc_pressure._type, "boundary_condition", "Type should be boundary_condition")
  assert_equal(bc_pressure.name, "inlet", "Name should be inlet")
  assert_equal(bc_pressure.condition_type, "pressure", "Type should be pressure")
  assert_equal(bc_pressure.value, 101325, "Value should be 101325")
  
  -- Test fluent API
  local bc_flow = Simulation.BoundaryCondition("outlet", "unknown")
  bc_flow:flow_rate(1e-6)
  
  assert_equal(bc_flow.condition_type, "flow_rate", "Should set flow_rate type")
  assert_equal(bc_flow.value, 1e-6, "Should set flow_rate value")
  
  print("✓ Test 10 PASSED: Boundary conditions")
end

-- =============================================================================
-- Test 11: Simulation State
-- =============================================================================

function Test.test_simulation_state()
  local state = Simulation.SimulationState()
  
  assert_equal(state._type, "simulation_state", "Type should be simulation_state")
  assert_equal(state.time, 0, "Initial time should be 0")
  assert_equal(state.iteration, 0, "Initial iteration should be 0")
  
  -- Test velocity/pressure setting
  state:set_velocity("edge_1", 0.5)
  state:set_pressure("node_1", 101325)
  
  assert_equal(state:get_velocity("edge_1"), 0.5, "Should get velocity back")
  assert_equal(state:get_pressure("node_1"), 101325, "Should get pressure back")
  
  -- Test time advancement
  state:advance_time(0.001)
  assert_equal(state.time, 0.001, "Time should be 0.001 after advance")
  assert_equal(state.iteration, 1, "Iteration should be 1 after advance")
  
  print("✓ Test 11 PASSED: Simulation state")
end

-- =============================================================================
-- Test 12: Simulation Engine Creation
-- =============================================================================

function Test.test_simulation_engine()
  local sim = Simulation.SimulationEngine("test_sim", {
    fluid_properties = Simulation.FluidProperties("water"),
  })
  
  assert_equal(sim._type, "simulation", "Type should be simulation")
  assert_equal(sim.name, "test_sim", "Name should be test_sim")
  assert_true(sim.timestep_config ~= nil, "Should have timestep config")
  assert_true(sim.solver_config ~= nil, "Should have solver config")
  assert_true(sim.fluid_properties ~= nil, "Should have fluid properties")
  assert_true(sim.state ~= nil, "Should have state")
  
  print("✓ Test 12 PASSED: Simulation engine creation")
end

-- =============================================================================
-- Test 13: Simulation Engine Fluent API
-- =============================================================================

function Test.test_simulation_fluent_api()
  local sim = Simulation.SimulationEngine("test_sim")
  
  local new_sim = sim
    :with_fluid(Simulation.FluidProperties("lymph"))
    :with_timestep(Simulation.TimeStepConfig({dt = 0.0001}))
    :with_solver(Simulation.SolverConfig("navier_stokes"))
  
  assert_equal(new_sim.fluid_properties.fluid_type, "lymph", "Fluid should be lymph")
  assert_equal(new_sim.timestep_config.dt, 0.0001, "dt should be 0.0001")
  assert_equal(new_sim.solver_config.solver_type, "navier_stokes", 
    "Solver should be navier_stokes")
  
  print("✓ Test 13 PASSED: Simulation fluent API")
end

-- =============================================================================
-- Test 14: Convenience Function - create()
-- =============================================================================

function Test.test_convenience_create()
  local sim = Simulation.create("lymph_sim", "lymph")
  
  assert_equal(sim.name, "lymph_sim", "Name should be lymph_sim")
  assert_equal(sim.fluid_properties.fluid_type, "lymph", "Fluid should be lymph")
  assert_true(sim.state ~= nil, "Should have state")
  
  print("✓ Test 14 PASSED: Convenience create()")
end

-- =============================================================================
-- Test 15: Convenience Function - stokes_solver()
-- =============================================================================

function Test.test_convenience_stokes_solver()
  local solver = Simulation.stokes_solver()
  
  assert_equal(solver.solver_type, "stokes", "Should be stokes")
  assert_equal(solver.method, "direct", "Method should be direct")
  assert_equal(solver.time_integration, "backward_euler", "Should use backward Euler")
  
  print("✓ Test 15 PASSED: Convenience stokes_solver()")
end

-- =============================================================================
-- Test 16: Convenience Function - navier_stokes_solver()
-- =============================================================================

function Test.test_convenience_navier_stokes_solver()
  local solver = Simulation.navier_stokes_solver()
  
  assert_equal(solver.solver_type, "navier_stokes", "Should be navier_stokes")
  assert_equal(solver.method, "iterative", "Method should be iterative")
  assert_equal(solver.time_integration, "bdf2", "Should use BDF2")
  
  print("✓ Test 16 PASSED: Convenience navier_stokes_solver()")
end

-- =============================================================================
-- Run All Tests
-- =============================================================================

function Test.run_all()
  print("\n" .. string.rep("=", 70))
  print("Running Simulation Module Tests")
  print(string.rep("=", 70) .. "\n")
  
  local tests = {
    "test_timestep_config",
    "test_solver_config",
    "test_fluid_properties_water",
    "test_fluid_properties_lymph",
    "test_fluid_properties_custom",
    "test_kinematic_viscosity",
    "test_channel_node",
    "test_channel_edge",
    "test_channel_network",
    "test_boundary_conditions",
    "test_simulation_state",
    "test_simulation_engine",
    "test_simulation_fluent_api",
    "test_convenience_create",
    "test_convenience_stokes_solver",
    "test_convenience_navier_stokes_solver",
  }
  
  local passed = 0
  local failed = 0
  local errors = {}
  
  for _, test_name in ipairs(tests) do
    if Test[test_name] then
      local success, err = pcall(Test[test_name])
      if success then
        passed = passed + 1
      else
        failed = failed + 1
        table.insert(errors, string.format("%s: %s", test_name, tostring(err)))
      end
    else
      error(string.format("Test function %s not found", test_name))
    end
  end
  
  print("\n" .. string.rep("=", 70))
  print(string.format("Results: %d passed, %d failed", passed, failed))
  print(string.rep("=", 70))
  
  if failed > 0 then
    print("\nErrors:")
    for _, error_msg in ipairs(errors) do
      print("  • " .. error_msg)
    end
    return false
  end
  
  return true
end

-- Run if executed as main script
if arg and arg[0] and arg[0]:match("test_simulation") then
  Test.run_all()
end

return Test
