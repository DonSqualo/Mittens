# Internal Representation Plan (Geometry First)

## Goal

Add a canonical intermediary geometry representation between Lua authoring and downstream adapters (mesh/export/physics), with deterministic checks to prevent silent divergence.

## Design Rules

- IR is the single geometry meaning layer.
- Existing mesh/export paths become adapters from IR.
- Canonicalization and hashing must be deterministic.
- Conformance is tolerance-based, not exact triangle equality.
- Biology-first expansion comes after geometry loop is stable.

## Phase 0: Baseline and Conventions

- freeze current authoring conventions from active projects
- document compatibility boundary for IR v0
- add a runnable quick loop on a real target (`pure_acoustics`)

Done in this change:

- convention extraction spec
- first quick loop subagent binary scaffold

## Phase 1: IR v0 Schema

- define canonical node kinds:
  - primitive
  - csg (`union`, `difference`, `intersect`)
  - group/assembly/component/instance
- normalize transform op encoding
- canonicalize params table order and numeric precision policy
- deterministic scene hash from canonical JSON

## Phase 2: Adapter Bridge

- `Lua -> scene table -> canonical IR snapshot`
- `canonical IR -> current Manifold mesh adapter` (existing geometry backend)
- persist artifacts:
  - canonical IR JSON
  - hash + counts
  - mesh validation report

## Phase 3: Conformance Hooks

- quick geometry checks:
  - bbox sanity
  - mesh triangle/vertex counts
  - degenerate triangle count
  - non-finite value detection
- tolerance oracle checks:
  - symmetric sampled surface distance (Hausdorff-style approximation)
  - inside/outside disagreement with boundary-band exclusion
  - sampled volume delta from shared bounding box
- add pre-commit/CI command path for quick loop on representative scenes

## Phase 4: Dual Adapter Expansion

- add OCCT/STEP adapter path fed by same canonical IR
- run same checks and add tolerance comparison report
- explicitly track known mismatch classes (boolean edge cases, tiny features)

## Phase 5: Interface-First Multiphysics

- add interface entities (ports/blankets) above geometry IR
- keep cell-agent/tissue models as separate typed layer using shared geometry domains
- preserve simple geometry-first core for device and manufacturing workflows
