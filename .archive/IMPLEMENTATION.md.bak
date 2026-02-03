# Helmholtz Coil Implementation Plan

## Paper Summary: Burd et al. 2025

**Goal:** RF magnetic field control of radical-pair reaction yields in RFP-flavin systems, demonstrated in vitro and in live C. elegans.

### Key Hardware from Paper

| Component | Specification |
|-----------|---------------|
| **Static field (B₀)** | 0–30 mT via Helmholtz coils |
| **B₀ uniformity** | <0.3% over ~(1.5 cm)³ |
| **RF resonator** | Bridged loop-gap resonator (BLGR) |
| **RF frequency** | ~450 MHz (tunable) |
| **RF field (B₁)** | 0.07–0.37 mT |
| **Q factor** | ~600 |
| **ESR condition** | B₀ = 15.9 mT @ f = 447 MHz |
| **Sample tube** | 3 mm ID quartz |
| **Blue laser** | 440 nm (photosensitization) |
| **Green laser** | 520 nm (excitation) |
| **Filter** | 650 nm longpass |

### Physics

ESR frequency: `f_ESR = g·μ_B·B₀/h`

At B₀ = 15.9 mT → f = 447 MHz (electron g ≈ 2.002)

---

## Current Setup (`helmholtz_coil.lua`)

| Parameter | Value |
|-----------|-------|
| Coil mean radius | 50 mm |
| Gap | 42.5 mm |
| Windings | 120 (12 layers × 10 turns) |
| Current | 2.0 A |
| Wire diameter | 0.8 mm |
| Center distance | ~55 mm |
| **Calculated B₀** | ~4.3 mT (ideal Helmholtz) |

### Additional Components (not in paper)

- Bridge-gap resonator (acoustic, not RF)
- Coupling coil (1 turn, for NanoVNA)
- PLA scaffold
- PTFE dielectric

---

## Divergences

### 1. **B-field Magnitude** — CRITICAL

| | Current | Required | Factor |
|-|---------|----------|--------|
| B₀ | ~4 mT | 16 mT | 4× |

**Options to increase B₀:**
- Increase current: 2A → 8A (thermal limits?)
- Increase windings: 120 → 480 (space constraints)
- Decrease radius: 50mm → 25mm (reduces uniformity region)
- Combination: e.g., 4A + 240 windings

### 2. **RF Resonator** — MISSING

Paper uses BLGR at 450 MHz. Current setup has no RF capability.

**Implementation needed:**
- Design BLGR with f_res ≈ 450 MHz
- Q factor target: 500–800
- B₁ field orientation perpendicular to B₀
- Sample positioned at B₁ maximum

### 3. **Sample Geometry** — DIFFERENT PURPOSE

Current: Bridge-gap resonator (acoustic/electromagnetic resonance)
Paper: Simple quartz tube for protein/nematode samples

**Decision point:** Are we replicating the paper setup or adapting it for ultrasound work?

### 4. **Optical Path** — NOT MODELED

Paper requires:
- 440 nm photosensitization (creates flavin photoproduct)
- 520 nm excitation (drives RFP fluorescence)
- 650 nm longpass filter (blocks excitation, passes RFP emission)

Current setup has no optical components.

### 5. **Helmholtz Ratio** — CLOSE BUT NOT IDEAL

Current: d/R ≈ 1.1 (deviation from ideal d/R = 1.0)
Paper: Optimized for uniformity

---

## Implementation Phases

### Phase 1: B₀ Field Redesign

**Goal:** Achieve 16+ mT with good uniformity

```lua
-- Target parameters
Coil = {
  mean_radius = 40,      -- mm (reduced for stronger field)
  windings = 200,        -- increased
  layers = 16,
  current = 4.0,         -- A (doubled)
}
-- Expected B₀ ≈ 16 mT
```

**Simulation tasks:**
- [ ] Calculate thermal dissipation (I²R losses)
- [ ] Model field uniformity over sample volume
- [ ] Optimize Helmholtz ratio for target uniformity <0.5%

### Phase 2: BLGR Design

**Goal:** 450 MHz resonator with Q > 500

Key parameters to model:
- Gap capacitance → resonant frequency
- Loop inductance → field distribution
- Bridge geometry → coupling strength
- Sample loading → Q degradation

**Reference:** Webb (1980), Froncisz & Hyde (1982) — classic BLGR papers

### Phase 3: Optical Integration

**Goal:** Model optical excitation paths

- Light pipe / fiber coupling geometry
- Filter holder positions
- Camera/detector placement

### Phase 4: Combined Simulation

**Goal:** Full multiphysics model

- Magnetostatics (B₀ from Helmholtz coils)
- RF electromagnetics (B₁ from BLGR)
- Thermal (coil heating)
- Optional: optical ray tracing

---

## Open Questions

1. **Purpose clarification:** Are we building the paper's setup exactly, or adapting it for ultrasound/ECM work?

2. **Scaling:** Paper works at ~450 MHz. If we want different frequencies (lower for deeper penetration?), how does geometry scale?

3. **Integration with existing resonator:** Can the bridge-gap resonator serve as the BLGR, or is it fundamentally different?

4. **Biological target:** C. elegans (paper) vs. cell cultures vs. tissue samples?

---

## References

- Burd et al. (2025) bioRxiv 10.1101/2025.02.27.640669 — This paper
- Webb (1980) — Loop-gap resonator theory
- Froncisz & Hyde (1982) — BLGR design
- Petryakov et al. (2007) — SLMG resonator (from Electron-MRI branch)
