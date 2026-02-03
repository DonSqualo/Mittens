# Lymph Project TODO

**Goal:** Fluid simulation in Mittens, rendered, with detailed CAD (aluminum extrusions + body analog)

## Progress Tracker
- **Started:** 2026-02-03 01:30 UTC
- **Target:** 16 sub-agent runs over 8 hours (every 30 min)
- **Cleanup stages:** 4, 8, 12, 16

## Stage Status

| # | Task | Status | Agent | Time |
|---|------|--------|-------|------|
| 1 | Fix renderer (verify geometry visible) | pending | - | - |
| 2 | Add aluminum extrusion profiles to stdlib | pending | - | - |
| 3 | Build bath frame with 80/20 extrusions | pending | - | - |
| 4 | **CLEANUP #1** - remove dead code, organize | pending | - | - |
| 5 | Add cooling system manifold geometry | pending | - | - |
| 6 | Add 3D gantry structure | pending | - | - |
| 7 | Create detailed body analog (tissue layers) | pending | - | - |
| 8 | **CLEANUP #2** - refactor, document | pending | - | - |
| 9 | Add fluid simulation module to stdlib | pending | - | - |
| 10 | Implement 2D acoustic field (standing wave) | pending | - | - |
| 11 | Connect acoustic field to renderer (colormap) | pending | - | - |
| 12 | **CLEANUP #3** - consolidate, test | pending | - | - |
| 13 | Implement channel network flow solver | pending | - | - |
| 14 | Add flow visualization (particles/arrows) | pending | - | - |
| 15 | Implement animation loop (time stepping) | pending | - | - |
| 16 | **FINAL CLEANUP** - polish, document, verify | pending | - | - |

## Task Details

### Stage 1: Fix Renderer
- Verify far plane fix (50000) works
- Ensure lymph_bath.lua geometry is visible
- Fix any remaining camera/view issues
- Take screenshot proving geometry renders

### Stage 2: Aluminum Extrusion Profiles
- Add `stdlib/extrusions.lua` with 80/20 style profiles
- Support 20x20, 40x40, 20x40 profiles
- Include T-slot geometry for accurate rendering
- Add corner brackets and joining hardware

### Stage 3: Bath Frame
- Replace simple box bath with extrusion frame
- 2000x600x400mm outer dimensions
- Proper corner joints
- Mounting points for gantry

### Stage 4: CLEANUP #1
- Remove unused code from Helmholtz branch
- Organize project structure
- Update imports/requires
- Fix any linting issues

### Stage 5: Cooling System
- Add manifold channels to bath walls
- Inlet/outlet ports
- Serpentine flow path for even cooling
- Material properties for aluminum + water

### Stage 6: 3D Gantry
- X/Y linear rail system above bath
- Z-axis probe mount
- Stepper motor placeholders
- Cable management features

### Stage 7: Body Analog
- Multi-layer tissue model
- Skin (outer membrane)
- Subcutaneous layer
- Muscle analog
- Embedded channel network (lymphatic)
- Proper viscoelastic material properties

### Stage 8: CLEANUP #2
- Refactor repeated code patterns
- Add documentation comments
- Create consistent naming conventions
- Remove debug code

### Stage 9: Fluid Simulation Module
- Add `stdlib/simulation.lua` (expand existing stub)
- Define simulation config schema
- Create solver interface
- Add time-stepping infrastructure

### Stage 10: 2D Acoustic Field
- Implement standing wave equation
- Two-source configuration
- Pressure field calculation
- Phase sweep for moving standing wave

### Stage 11: Acoustic Renderer Integration
- Add field overlay to Three.js renderer
- Colormap for pressure magnitude
- Update on each frame
- XZ plane slice display

### Stage 12: CLEANUP #3
- Consolidate simulation code
- Add unit tests where possible
- Document API
- Performance optimization

### Stage 13: Channel Flow Solver
- 1D Poiseuille flow in channel network
- Acoustic body force coupling
- Pressure boundary conditions
- Flow rate calculation

### Stage 14: Flow Visualization
- Particle system for flow viz
- Arrow field option
- Color by velocity magnitude
- Animate with simulation timestep

### Stage 15: Animation Loop
- Implement time stepping in renderer
- Sync acoustic field with time
- Update flow visualization
- FPS-independent animation

### Stage 16: FINAL CLEANUP
- Full code review
- Remove all dead code
- Complete documentation
- Verify all features work together
- Final screenshot/demo

## Notes
- Each sub-agent gets ONE task
- Must take screenshot after changes
- Must pass verification before completing
- Write to memory on completion
