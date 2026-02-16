# Subagent Quick Loop: Pure Acoustics Geometry

## Purpose

Create a fast, repeatable loop that a subagent can run to:

- execute a Lua model
- extract canonical geometry IR JSON
- compute deterministic scene hash
- run current Manifold mesh generation and validation
- write artifacts for diff-based iteration

## Initial Target

- `examples/multiphysics/pure_acoustics.lua`

This model includes nested CSG, grouped assemblies, material tagging, exports, and study wiring, which makes it a good first stress target.

## Binary

- `server/src/bin/ir_subagent.rs`

## Conformance Mode (Cross-Backend)

`ir_subagent` can compare the Manifold mesh (generated from IR) against a candidate STL from another backend (for example OCCT STEP -> tessellated STL).

Metrics currently emitted:

- symmetric sampled surface distance (Hausdorff-style approximation)
- inside/outside disagreement over deterministic volume samples
- disagreement outside boundary band (robust mismatch signal)
- approximate volume delta from shared sampled bbox
- bounds deltas

Pass/fail is controlled by threshold flags.

## Command

From repo root:

```bash
cd server
cargo run --release --bin ir_subagent -- --file ../examples/multiphysics/pure_acoustics.lua
```

External project parity check:

```bash
cd server
cargo run --release --bin ir_subagent -- \
  --file /home/heim/projects/pure-acoustics/pure_acoustics.lua \
  --out-dir target/ir_subagent_projects_pure
```

Cross-backend conformance check (example with baseline self-check):

```bash
cd server
cargo run --release --bin ir_subagent -- \
  --file ../examples/multiphysics/pure_acoustics.lua \
  --emit-baseline-stl \
  --candidate-stl target/ir_subagent/baseline_manifold.stl
```

## Artifacts

Default output directory:

- `server/target/ir_subagent/`

Generated files:

- `canonical_ir.json` - canonicalized scene geometry tree
- `summary.json` - hash, object counts, mesh stats, validation summary
- `conformance.json` - tolerance oracle report (when `--candidate-stl` is provided)
- `baseline_manifold.stl` - baseline manifold mesh (when `--emit-baseline-stl` is set)

## First-Loop Success Criteria

- command completes without Lua/mesh errors
- canonical IR file is deterministic across repeated runs
- summary includes stable scene hash and mesh validation output

## Notes

- This loop is intentionally geometry-first.
- It does not replace the live websocket workflow.
- It is a stepping stone for future tolerance checks against additional adapters.
