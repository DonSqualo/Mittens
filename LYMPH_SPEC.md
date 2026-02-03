# Lymph Bath Simulation Spec

## Reference
Hauglund et al. 2025 (Cell) — Norepinephrine-mediated slow vasomotion drives glymphatic clearance during sleep

**Key finding:** ~0.02 Hz infraslow oscillations drive CSF flow via vasomotion (arterial wall pumping). We ask: can external acoustic forcing achieve the same?

---

## Architecture Constraint

```
Lua Script → CSG → Mesh → [SIMULATION] → Renderer (animated)
                              ↑
                         NEW LAYER
```

The mesh is the single source of truth. Simulation layer:
- Reads mesh geometry
- Applies physics (acoustic field + fluid flow)
- Outputs time-varying fields for renderer
- Displayed on XZ plane (2D slice)

---

## Physical Model (2D, XZ plane)

### Domain
```
X: 0 → 2000 mm (bath length)
Z: 0 → 400 mm (bath depth)

┌────────────────────────────────────────────────┐
│                   Water                         │
│   ┌────────────────────────────────────────┐   │
│   │        Gel (tissue surrogate)          │   │
│   │   ════════════════════════════════     │   │ ← channels
│   │      ══════════════════════════        │   │
│   │   ════════════════════════════════     │   │
│   └────────────────────────────────────────┘   │
│ [S]                                        [S] │ ← speakers
└────────────────────────────────────────────────┘
```

### Scales
| Domain | Physics | Equation |
|--------|---------|----------|
| Water | Linear acoustics | ∇²p + k²p = 0 |
| Gel | Viscoelastic + streaming | Kelvin-Voigt + Stokes |
| Channels | 1D Poiseuille + compliance | Q = -πR⁴/8η · ∂p/∂x |

### Material Properties (from Hauglund context)

**Water (37°C)**
- ρ = 993 kg/m³
- c = 1524 m/s
- Z = 1.51 MRayl

**Gel (tissue surrogate)**
- ρ = 1040 kg/m³
- c = 1540 m/s
- G' = 2 kPa (storage modulus at 0.02 Hz)
- G'' = 0.2 kPa (loss modulus)
- α = 0.5 dB/cm/MHz

**Lymph fluid**
- ρ = 1020 kg/m³
- η = 1.8 mPa·s

**Channel network (simplified)**
- Main trunk: d = 3 mm
- Secondary: d = 1 mm
- Collectors: d = 0.3 mm

---

## Acoustic Forcing

### Baseline: Standing wave (two speakers)
```lua
frequency = 0.02  -- Hz (match natural vasomotion)
amplitude = 100   -- Pa
phase_sweep = 0.1 -- Hz (moving standing wave rate)
```

### Standing wave equation
```
p(x,t) = 2A · cos(kx) · cos(ωt)         -- pure standing
p(x,t) = 2A · cos(kx - Ωt/2) · cos(ωt)  -- moving (phase sweep)
```

### Parameter space
- f: 0.01 - 1 Hz (infraslow regime from paper)
- A: 10 - 1000 Pa
- Ω: 0 - 0.5 Hz (sweep rate)

---

## Simulation Pipeline

### Step 1: Geometry (existing Mittens)
Lua script outputs mesh with tagged regions:
- `bath_water`
- `gel_matrix`
- `channel_trunk`
- `channel_secondary[]`

### Step 2: Acoustic field
Compute p(x,z,t) in bath + gel:
- Helmholtz solve or analytic standing wave
- Include attenuation in gel
- Compute acoustic streaming force: F = -⟨ρv·∇v⟩

### Step 3: Fluid solve
In channels:
- 1D network flow with acoustic body force
- Compliant walls (optional, V2)
- Output: Q(t) per channel, p(x,t) along channels

### Step 4: Render
- XZ plane view
- Pressure field as color map
- Channel flow as animated particles or arrows
- Gel displacement as deformation (optional)

---

## Renderer Integration

### Time-varying fields
Renderer needs:
```rust
struct SimulationFrame {
    time: f32,
    pressure_field: Vec<f32>,   // 2D grid values
    channel_flow: Vec<f32>,     // Q per channel segment
    // Optional:
    gel_displacement: Vec<Vec2>,
    particle_positions: Vec<Vec2>,
}
```

### Animation loop
```
t = 0
while running:
    frame = simulation.step(dt)
    renderer.update_fields(frame)
    renderer.draw()
    t += dt
```

### Display on XZ
- Camera: orthographic, looking down Y axis
- Domain: X = [0, 2000mm], Z = [0, 400mm]
- Colormap: pressure magnitude (viridis or similar)

---

## Implementation Plan

### Phase 1: Geometry + static render
- [ ] `project/lymph_bath.lua` — bath + gel + channels (2D slice)
- [ ] Verify XZ plane rendering

### Phase 2: Acoustic field
- [ ] `stdlib/simulation.lua` — simulation config DSL
- [ ] `server/src/acoustic.rs` — standing wave solver
- [ ] Pressure field → renderer

### Phase 3: Channel flow
- [ ] 1D network solver (Rust)
- [ ] Flow visualization (arrows or particles)

### Phase 4: Animation
- [ ] Time stepping in renderer
- [ ] Moving standing wave visualization
- [ ] Flow response to acoustic forcing

### Phase 5: Optimization
- [ ] Parameter sweep harness
- [ ] Objective: net flow rate vs power

---

## Open Questions → Heye

1. **Frequency?** Paper shows 0.02 Hz natural. Start there or sweep wider?
2. **Channel topology?** Schematic (3-5 parallel channels) or anatomical?
3. **Render style?** Colormap + arrows, or particle tracing?

---

## Files

```
project/lymph_bath.lua     — Main geometry + simulation setup
stdlib/simulation.lua      — Simulation DSL (if needed)
server/src/simulation/     — Rust solvers
  acoustic.rs              — Standing wave field
  channel_flow.rs          — 1D network flow
  mod.rs
```
