# Geometry and Authoring Conventions (Observed)

This document captures how current models are authored so IR v0 matches real usage.

## Sources Reviewed

- `examples/multiphysics/pure_acoustics.lua`
- `examples/multiphysics/bridge_gap_resonator.lua`
- `project/helmholtz_coil.lua`
- `~/projects/pure-acoustics/pure_acoustics.lua`
- `~/projects/helmholtz/helmholtz_coil.lua`
- `~/projects/hallbach/hallbach.lua`
- `~/projects/lymph-clean/lymph_clean.lua`
- `~/projects/ICD/ICD.lua`

## Core Geometry Patterns

- Primitives are mostly `box`, `cylinder`, `sphere`, `ring`, `torus`.
- Shells are commonly built as `difference(outer, inner_with_height_plus_one)`.
- Complex parts are usually nested `difference/union` over cylinders and boxes.
- Repetition patterns are authored with Lua loops that generate arrays of cuts/features.

## Transform and Frame Patterns

- Method chaining is dominant: `:center(...)`, `:rotate(...)`, `:at(...)`.
- Rotation is Euler degrees in `x,y,z`.
- Models use both origin-at-base and center-aligned placement through `:center("XY"/"XYZ")`.
- Z is used as the primary stack axis in fixtures and transducer setups.

## Grouping and Naming Patterns

- Top-level return pattern is `Mittens.serialize()`.
- Scene uses named `group(...)` assemblies.
- Component tables are usually PascalCase (`Coil`, `Bridge`, `Transducer`) with `.model`/`.body`.
- Export intent is explicit with `export_stl(...)` calls per manufacturable subassembly.

## Physics and Instrument Coupling Patterns

- Geometry and studies are colocated in one file.
- Studies reference geometry objects directly (`transducer = Transducer.model`).
- Instrument and circuit blocks are first-class and registered like geometry.

## Compatibility Boundary for IR v0

Keep compatible now:

- primitive params
- CSG operation trees
- ordered transform ops
- groups/assemblies/components
- material/color/name fields needed by renderer/export

Not required in v0:

- perfect backwards compatibility for every legacy helper idiom
- non-geometry simulation internals
- deprecated call forms from old scripts
