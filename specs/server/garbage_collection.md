# Garbage Collection

Post-project review process. After completing a project, run through these questions to decide what survives.

## For Each Piece of Project Code

### 1. Core Pipeline Changes

Did you modify any of these files?
- `server/src/geometry.rs`
- `server/src/export.rs`
- `server/src/main.rs`
- `renderer/src/main.ts`
- `stdlib/*.lua`

**If yes, ask:**
- Was this a bug fix in existing functionality? → **Merge to core**
- Was this a new capability the pipeline was missing? → **Ask user:**
  - "I added [X] to [file]. Should this become part of core, or should I revert and document in specs?"
- Was this project-specific rendering/export? → **Revert, document in specs if noteworthy**

### 2. Generated Primitives

Did tooling generate new primitives (torus, sphere, wedge, etc.)?

**Always discard the code.** But ask:
- Did generating this primitive reveal a gotcha? → **Add to gotchas.md**
- Was the math non-obvious? → **Document the formula in a project spec for faster regeneration**

### 3. Multiphysics / Simulation Code

Did the project add physics simulations?

**Ask user:**
- "This project included [magnetic field sim / ultrasound / circuit / etc.]. Options:"
  - Delete entirely (default)
  - Write learnings to `specs/projects/[project_name].md`
  - Promote to core (rare - only if it's foundational)

### 4. Project Lua Files

The `.lua` files in `examples/` or project directories.

**Ask user:**
- "Keep `[file.lua]` as an example, or delete?"
- If keeping: "Should I strip it to minimal demonstration, or keep full complexity?"

### 5. Generated Outputs

STL, STEP, 3MF files, simulation results.

**Ask user:**
- "Delete generated files, or archive to [location]?"

## Questions Template

After a project, contributors should ask:

```
Project complete. Garbage collection review:

1. PIPELINE CHANGES
   - [list modified core files, if any]
   - Recommendation: [merge/revert/ask]

2. GENERATED PRIMITIVES
   - [list any new primitives created]
   - Any gotchas to document? [y/n]

3. PHYSICS/SIMULATION
   - [list simulation types used]
   - Write project spec? [y/n]

4. PROJECT FILES
   - [list .lua files]
   - Keep as example? [y/n per file]

5. OUTPUTS
   - [list generated files]
   - Archive or delete?

What would you like to keep?
```

## What NEVER Gets Kept in Code

- Project-specific primitives
- One-off helper functions
- Visualization hacks
- Hardcoded dimensions/parameters

## What MIGHT Get Kept in Code

- Bug fixes to manifold generation
- Missing export format support
- Renderer crash fixes
- Performance improvements to core pipeline

## What Gets Kept in Specs

- Gotchas discovered during the project
- Non-obvious mathematical formulas
- Simulation validation learnings (what matched reality, what didn't)
- Iteration patterns that worked well
