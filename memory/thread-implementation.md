# ISO Metric Thread Implementation in Mittens

## Overview
Implemented ISO 68-1 metric threads in Rust (geometry.rs) with Lua bindings (stdlib/threads.lua).

## Thread Geometry (ISO 68-1)

### Key Dimensions
- **Thread depth**: `0.54125 * pitch` (for 60° V-thread)
- **Major radius**: `major_diameter / 2`
- **Minor radius**: `major_radius - thread_depth`

### Profile Shape
Trapezoidal approximation of 60° V-thread:
- `thread_angle_factor = 0.577` (tan 30°)
- `crest_half_width = thread_depth * thread_angle_factor * 0.5` (narrower at peaks)
- `root_half_width = pitch * 0.45` (wider at valleys)

## Helical Mesh Generation

### Approach
Generate closed helical solid by sweeping a quad profile along a helix:

```
Profile vertices per segment (4 points):
0: inner-bottom (minor_radius, z - root_half_width)
1: outer-bottom (major_radius, z - crest_half_width)  
2: outer-top (major_radius, z + crest_half_width)
3: inner-top (minor_radius, z + root_half_width)
```

### Helix Parameters
```rust
let num_turns = height / pitch;
let total_segments = ((num_turns + 1.0) * segments_per_turn).ceil() as usize;

for seg in 0..=total_segments {
    let t = seg as f64 / segments_per_turn as f64;
    let angle = t * 2π;
    let z_center = t * pitch - half_pitch;
    // Generate 4 profile vertices at this angle/z
}
```

### Triangle Generation
1. **Start cap**: Close first quad profile with 2 triangles
   ```rust
   tri_verts.extend_from_slice(&[0, 2, 1]);
   tri_verts.extend_from_slice(&[0, 3, 2]);
   ```

2. **Side faces**: Connect consecutive profile rings
   ```rust
   for seg in 0..total_segments {
       for i in 0..4 {
           let next_i = (i + 1) % 4;
           // Two triangles per quad face
           tri_verts.extend_from_slice(&[base + i, base + next_i, next + next_i]);
           tri_verts.extend_from_slice(&[base + i, next + next_i, next + i]);
       }
   }
   ```

3. **End cap**: Close last quad profile (opposite winding)
   ```rust
   tri_verts.extend_from_slice(&[last + 0, last + 1, last + 2]);
   tri_verts.extend_from_slice(&[last + 0, last + 2, last + 3]);
   ```

## Edge Closing Fix

### Problem
Original implementation used fan-triangles from profile vertices to a single center point. This created non-manifold geometry because:
- Center point at one radius (minor)
- Profile spans two radii (minor and major)
- Fan triangles don't close the quad cross-section properly

### Solution
Changed caps from fan-triangles to proper quad triangulation:
- 4-vertex quad → 2 triangles (diagonal split)
- Start cap: `[0,2,1], [0,3,2]`
- End cap: `[0,1,2], [0,2,3]` (opposite winding)

## Tolerance / Clearance (ISO Classes)

### Female Thread (Class 6H)
- **Reference dimension** - stays at nominal
- Bore at `major_diameter`
- Crests inward to `minor_radius`

### Male Thread (Class 6g)
- **Undersized from nominal** for clearance
- `clearance` parameter (default 0, recommend 0.2mm for 3D printing)
- Crests at `major_radius - clearance`
- Roots at `minor_radius - clearance`

### Why Male Gets Clearance
1. Female thread is the "reference" in ISO standards (easier to gauge)
2. Male threads easier to post-process (file/sand down if too tight)
3. For 3D printing: layer lines add roughness, male is easier to fix

### Recommended Clearance Values
- **Machined parts**: 0.1-0.15mm
- **3D printed (FDM)**: 0.2-0.25mm
- **3D printed (SLA)**: 0.15-0.2mm

## Boolean Operations

### External Thread
```rust
// Core cylinder with overlap for solid merge
let core = cylinder(height, minor_radius + 0.05);

// Union core with helical thread mesh
let unioned = core.union(&thread_mesh);

// Trim to height bounds
let bound = cylinder(height, major_radius + 0.1);
let result = unioned.intersection(&bound);
```

### Internal Thread
```rust
// Tube: outer cylinder minus inner bore at major
let tube = cylinder(height, outer_radius)
    .difference(&cylinder(height, major_radius));

// Union tube with inward-pointing thread mesh
let unioned = tube.union(&thread_mesh);

// Trim, then clear center bore at minor
let trimmed = unioned.intersection(&bound);
let result = trimmed.difference(&cylinder(height, minor_radius));
```

## Mesh Quality Notes

### Multiple Shells
The helical generation creates separate shells per turn that touch but don't merge into single mesh. This is acceptable:
- admesh reports "Number of parts" > 1
- All shells are watertight (0 disconnected edges)
- Slicers (PrusaSlicer, Cura) merge overlapping geometry

### Degenerate Facets
Boolean operations create thin slivers at trim boundaries. These are:
- Automatically removed by admesh repair
- Don't affect printability

## Lua API

```lua
-- External (male) thread
Threads.external({
  major_diameter = 27,  -- mm
  pitch = 0.75,         -- mm
  height = 10,          -- mm
  segments_per_turn = 32,  -- resolution
  clearance = 0.2       -- mm, for 3D printing
})

-- Internal (female) thread  
Threads.internal({
  major_diameter = 27,
  pitch = 0.75,
  height = 10,
  segments_per_turn = 32
  -- no clearance param, stays at nominal
})
```

## Files
- `server/src/geometry.rs`: Rust mesh generation (`external_thread`, `internal_thread`)
- `stdlib/threads.lua`: Lua API and ISO specs table
- `project/insert.lua`: Example M27x0.75 male/female pair
