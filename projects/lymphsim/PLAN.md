# LymphSim CAD Integration Plan

**Goal:** 2m lymphatic drainage bath simulation integrated into Mittens CAD/renderer.
**Target:** 10x acceleration of glymphatic clearance via moving standing waves.
**Status:** ACTIVE

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  lymphsim.lua                                                    │
│  - Chamber geometry (2m bath, human model)                       │
│  - Vessel networks (vascular, glymphatic, lymphatic)             │
│  - Acoustic sources (speakers at ends)                           │
│  - Simulation parameters                                         │
└─────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│  stdlib/physics/fluids.lua (NEW)                                 │
│  - vessel_network(): Create tube network with flow               │
│  - fluid_tracer(): Colored particles/concentration               │
│  - acoustic_field(): Standing wave pressure field                │
│  - moving_standing_wave(): Drifting node pattern                 │
└─────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│  Rust Backend Extension                                          │
│  - FluidSimState: Time-stepped simulation state                  │
│  - Mesh updates per frame for animation                          │
│  - Tracer particle positions                                     │
└─────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│  Renderer Extensions                                             │
│  - Animated tube colors (tracer concentration)                   │
│  - Pressure field visualization (optional heatmap plane)         │
│  - Time controls (play/pause/speed)                              │
│  - Metrics overlay (clearance time, flow rates)                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Phase Breakdown

### Phase 1: Static Geometry
**Owner:** Sub-agent
**Deliverables:**
- [ ] `projects/lymphsim/chamber.lua` - Bath geometry
- [ ] `projects/lymphsim/human_2d.lua` - Simplified human cross-section  
- [ ] `projects/lymphsim/vessels.lua` - 3-compartment vessel network
- [ ] Screenshot verification

### Phase 2: Fluid Primitives
**Owner:** Sub-agent
**Deliverables:**
- [ ] `stdlib/physics/fluids.lua` - vessel_network(), fluid properties
- [ ] Tube rendering with per-segment colors
- [ ] Tracer concentration visualization
- [ ] Screenshot verification

### Phase 3: Acoustic Field
**Owner:** Sub-agent
**Deliverables:**
- [ ] Acoustic source primitives
- [ ] Standing wave pressure field calculation
- [ ] Moving standing wave (drifting phase)
- [ ] Pressure heatmap plane visualization
- [ ] Screenshot verification

### Phase 4: Physics Coupling
**Owner:** Sub-agent  
**Deliverables:**
- [ ] Pressure → vessel wall displacement
- [ ] Wall motion → fluid pumping (Hagen-Poiseuille)
- [ ] Interface flows (blood→CSF→lymph)
- [ ] Tracer advection
- [ ] Screenshot verification

### Phase 5: Animation Loop
**Owner:** Sub-agent
**Deliverables:**
- [ ] Time-stepped simulation in Rust backend
- [ ] WebSocket frame updates
- [ ] Renderer animation handling
- [ ] Play/pause controls
- [ ] Screenshot verification

### Phase 6: Optimization & Metrics
**Owner:** Sub-agent
**Deliverables:**
- [ ] Parameter sweep infrastructure
- [ ] Clearance time measurement
- [ ] Metrics overlay in renderer
- [ ] Find 10x speedup configuration
- [ ] Final verification

---

## File Structure

```
Mittens/
├── projects/
│   └── lymphsim/
│       ├── PLAN.md          # This file
│       ├── main.lua         # Entry point
│       ├── chamber.lua      # Bath geometry
│       ├── human_2d.lua     # Human cross-section
│       ├── vessels.lua      # Vessel networks
│       └── simulation.lua   # Physics parameters
├── stdlib/
│   └── physics/
│       └── fluids.lua       # Fluid simulation primitives
└── server/
    └── src/
        └── fluid_sim.rs     # Rust simulation backend (if needed)
```

---

## Key Physics

**Pressure cascade:**
```
Blood (13 kPa) → CSF (1.3 kPa) → Lymph (0.4 kPa)
```

**Flow equation (Hagen-Poiseuille):**
```
Q = (π r⁴ / 8μL) × ΔP
```

**Moving standing wave:**
```
p(x,t) = 2A × cos(kx - Ωt/2) × cos(ωt - Ωt/2)
Node velocity = Ω / 2k
```

**Target:** Baseline ~10s clearance → <1s with optimization

---

## Sub-Agent Instructions

Each sub-agent should:
1. Read this PLAN.md first
2. Check off completed items
3. Take screenshot after EVERY code change
4. Report back with: `<phase>N</phase><next>M</next>` or `<blocked>reason</blocked>`
5. Update this file with progress notes

---

## Progress Log

### 2026-02-02
- Initial Python prototype showed 5.3x speedup possible
- Starting CAD integration
