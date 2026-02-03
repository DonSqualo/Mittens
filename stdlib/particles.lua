-- Mittens Standard Library: Particle Visualization for Flow
-- Visualizes flow rates through channels using animated particles
--
-- Architecture:
--   - Particle: Individual tracer that moves along a channel path
--   - ParticleSystem: Container managing all particles for a network
--   - Each particle has position, velocity, lifetime, flow magnitude
--   - Particles spawn at channel inlets, move along edges, respawn when reaching outlets
--   - Color-coded by flow magnitude (blue=slow, red=fast)

local Particles = {}

-- =============================================================================
-- Particle
-- =============================================================================

local function Particle(config)
  config = config or {}
  
  local particle = {
    _type = "particle",
    id = config.id or 1,
    
    -- Position along edge: [x, y, z] in mm
    position = config.position or {0, 0, 0},
    
    -- Travel parameter: 0 = at source node, 1 = at target node
    progress = config.progress or 0.0,
    
    -- Edge ID this particle is currently on
    current_edge = config.current_edge,
    
    -- Source and target nodes for current edge
    source_node = config.source_node,
    target_node = config.target_node,
    
    -- Flow magnitude at this particle (for coloring)
    flow_magnitude = config.flow_magnitude or 0,
    
    -- Particle speed (mm/s) - proportional to flow rate
    speed = config.speed or 1.0,
    
    -- Lifetime: 0-1 (0=just spawned, 1=should respawn)
    life = config.life or 0.0,
  }
  
  setmetatable(particle, {__index = {
    -- Update particle position by advancing along edge
    update = function(self, dt)
      if not self.source_node or not self.target_node then
        return self
      end
      
      -- Distance to travel (mm)
      local src = self.source_node.position
      local tgt = self.target_node.position
      local dx = tgt[1] - src[1]
      local dy = tgt[2] - src[2]
      local dz = tgt[3] - src[3]
      local edge_length = math.sqrt(dx*dx + dy*dy + dz*dz)
      
      -- Advance progress along edge
      if edge_length > 0 then
        self.progress = self.progress + (self.speed * dt) / edge_length
      end
      
      -- Update interpolated position
      self.position[1] = src[1] + dx * self.progress
      self.position[2] = src[2] + dy * self.progress
      self.position[3] = src[3] + dz * self.progress
      
      -- Update lifetime
      self.life = self.progress
      
      return self
    end,
    
    -- Check if particle has reached end of edge
    is_done = function(self)
      return self.progress >= 1.0
    end,
    
    -- Get color based on flow magnitude
    -- Blue (slow) → Green → Yellow → Red (fast)
    get_color = function(self, max_magnitude)
      max_magnitude = max_magnitude or 1.0
      local normalized = math.min(1.0, self.flow_magnitude / max_magnitude)
      
      -- Color map: blue at 0, cyan at 0.25, green at 0.5, yellow at 0.75, red at 1
      local r, g, b
      if normalized < 0.25 then
        -- Blue to Cyan
        local t = normalized / 0.25
        r = t
        g = t
        b = 1.0
      elseif normalized < 0.5 then
        -- Cyan to Green
        local t = (normalized - 0.25) / 0.25
        r = 0
        g = 1.0
        b = 1.0 - t
      elseif normalized < 0.75 then
        -- Green to Yellow
        local t = (normalized - 0.5) / 0.25
        r = t
        g = 1.0
        b = 0
      else
        -- Yellow to Red
        local t = (normalized - 0.75) / 0.25
        r = 1.0
        g = 1.0 - t
        b = 0
      end
      
      return {r, g, b, 1.0}
    end,
    
    serialize = function(self)
      return {
        id = self.id,
        position = self.position,
        progress = self.progress,
        flow_magnitude = self.flow_magnitude,
        speed = self.speed,
        life = self.life,
      }
    end
  }})
  
  return particle
end

-- =============================================================================
-- Particle System
-- =============================================================================

local function ParticleSystem(config)
  config = config or {}
  
  local system = {
    _type = "particle_system",
    particles = {},  -- List of active particles
    particle_count = 0,
    next_particle_id = 1,
    
    -- Configuration
    particles_per_inlet = config.particles_per_inlet or 3,
    max_particles = config.max_particles or 100,
    
    -- Reference data
    channel_network = config.channel_network,
    simulation_state = config.simulation_state,
    
    -- Time accumulation for spawning
    spawn_timer = 0,
    spawn_interval = config.spawn_interval or 0.1,  -- seconds
    
    max_flow_magnitude = 1e-8,  -- For color normalization
  }
  
  setmetatable(system, {__index = {
    -- Initialize particles at inlet channels
    initialize = function(self)
      if not self.channel_network then
        return self
      end
      
      -- Find inlet channels (sources with flow_rate boundary condition)
      local inlets = {}
      for node_id, node in pairs(self.channel_network.nodes) do
        if node.node_type == "inlet" or node.flow_rate then
          table.insert(inlets, node_id)
        end
      end
      
      -- If no explicit inlets, use first node
      if #inlets == 0 then
        for node_id, _ in pairs(self.channel_network.nodes) do
          table.insert(inlets, node_id)
          break
        end
      end
      
      -- Spawn particles at each inlet
      for _, inlet_id in ipairs(inlets) do
        for i = 1, self.particles_per_inlet do
          self:spawn_particle(inlet_id)
        end
      end
      
      return self
    end,
    
    -- Spawn a new particle at a given node
    spawn_particle = function(self, node_id)
      if not self.channel_network then return self end
      if self.particle_count >= self.max_particles then return self end
      
      local node = self.channel_network:get_node(node_id)
      if not node then return self end
      
      -- Find an outgoing edge from this node
      local outgoing_edge = nil
      local next_node = nil
      
      for edge_id, edge in pairs(self.channel_network.edges) do
        if edge.source_node == node_id then
          outgoing_edge = edge
          next_node = self.channel_network:get_node(edge.target_node)
          break
        end
      end
      
      if not outgoing_edge or not next_node then
        return self
      end
      
      -- Calculate flow magnitude
      local flow_mag = math.abs(outgoing_edge.flow_rate or 0)
      self.max_flow_magnitude = math.max(self.max_flow_magnitude, flow_mag)
      
      -- Calculate particle speed based on flow rate
      -- Q = v * A, where v is velocity, A is cross-sectional area
      local radius = (outgoing_edge.diameter / 2.0) / 1000.0  -- Convert mm to m
      local area = math.pi * radius * radius  -- m^2
      local velocity = 0
      if area > 0 then
        velocity = flow_mag / area  -- m/s
      end
      velocity = velocity * 1000.0  -- Convert to mm/s
      
      local particle = Particle({
        id = self.next_particle_id,
        position = {node.position[1], node.position[2], node.position[3]},
        progress = 0.0,
        current_edge = edge_id,
        source_node = node,
        target_node = next_node,
        flow_magnitude = flow_mag,
        speed = math.max(1.0, velocity),  -- At least 1 mm/s
        life = 0.0,
      })
      
      table.insert(self.particles, particle)
      self.particle_count = self.particle_count + 1
      self.next_particle_id = self.next_particle_id + 1
      
      return self
    end,
    
    -- Update all particles
    update = function(self, dt)
      dt = dt or 0.001  -- Default to 1ms timestep
      
      -- Update spawn timer
      self.spawn_timer = self.spawn_timer + dt
      
      -- Update existing particles
      local living_particles = {}
      for _, particle in ipairs(self.particles) do
        particle:update(dt)
        
        if particle:is_done() then
          -- Particle reached end of edge, try to move to next edge
          local target_id = particle.target_node.id
          if not self:spawn_particle(target_id) then
            -- Failed to spawn at next location, particle dies
          end
        else
          -- Particle is still alive
          table.insert(living_particles, particle)
        end
      end
      
      self.particles = living_particles
      self.particle_count = #living_particles
      
      -- Spawn new particles at regular intervals
      if self.spawn_timer >= self.spawn_interval then
        self:initialize()
        self.spawn_timer = 0
      end
      
      return self
    end,
    
    -- Get all particles as serialized data
    serialize = function(self)
      local particles_data = {}
      for i, particle in ipairs(self.particles) do
        local color = particle:get_color(self.max_flow_magnitude)
        particles_data[i] = {
          id = particle.id,
          position = particle.position,
          flow_magnitude = particle.flow_magnitude,
          color = color,
        }
      end
      
      return {
        type = "particle_system",
        particle_count = self.particle_count,
        max_flow_magnitude = self.max_flow_magnitude,
        particles = particles_data,
      }
    end,
  }})
  
  return system
end

-- =============================================================================
-- Public API
-- =============================================================================

Particles.Particle = Particle
Particles.ParticleSystem = ParticleSystem

function Particles.create(config)
  return ParticleSystem(config)
end

return Particles
