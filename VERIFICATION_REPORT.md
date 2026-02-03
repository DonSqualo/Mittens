# Mittens Lymph Simulation - Verification Report
**Date:** 2026-02-03
**Verifier:** Sky (sub-agent)

## Executive Summary

**⚠️ CRITICAL: Most claimed features are NOT rendering properly.**

The mesh only generates **576 vertices** when it should have thousands. Only 2-3 objects appear to render (gel block, acoustic field overlay). The remaining ~12 objects (bath frame, channels, speakers, extrusions) are either:
1. Not being processed by CSG
2. Being culled/simplified by Manifold union
3. Invisible due to X-ray material + colors

---

## Feature Verification

### 1. Bath Frame (Aluminum Extrusion Structure)
**Status:** ❌ BROKEN
**Evidence:** Not visible in any screenshot
**Issue:** The `bath_shell` CSG difference IS processed (logs show `type=csg`), but the resulting mesh vertices seem to be merged/lost in the final union. The aluminum gray color may be invisible with X-ray shader.

### 2. Multi-Layer Tissue (Skin/Fat/Muscle)
**Status:** ❌ NOT IMPLEMENTED
**Evidence:** Code only has single `gel_block` box - no tissue layers
**Issue:** The Lua code defines ONE gel block, not multi-layer tissue. This feature was never implemented.

### 3. Lymphatic Channels (Green Tubes)
**Status:** ❌ BROKEN  
**Evidence:** Not visible in screenshots despite server processing transforms
**Issue:** Cylinders are being created (transforms applied in logs) but either:
- Too small (3mm diameter in 2000mm bath = 0.15% of scene)
- Lost in Manifold union operation
- Green color invisible with X-ray shader

### 4. 3D Gantry (X/Y/Z Rails)
**Status:** ❌ NOT IMPLEMENTED
**Evidence:** No gantry code in lymph_bath.lua
**Issue:** This feature was never coded.

### 5. Cooling Manifold (Serpentine Channels)
**Status:** ❌ NOT IMPLEMENTED
**Evidence:** No cooling manifold code exists
**Issue:** This feature was never coded.

### 6. Acoustic Field Visualization
**Status:** ⚠️ PARTIAL
**Evidence:** Purple/blue gradient visible in screenshots
**Issue:** 
- Acoustic field DOES render as XZ plane overlay ✅
- Uses AcousticField data from Lua ✅
- But it's just a static texture, not animated
- Colors show pressure distribution but no standing wave animation

### 7. Flow Particles (Animated)
**Status:** ❌ BROKEN
**Evidence:** No particles visible in screenshots
**Issue:** 
- ParticleSystem IS created in Lua (12 particles, 12 nodes, 11 edges)
- Renderer HAS particle code (`create_particles()`, `update_particles()`)
- But particles are not showing up - likely camera/scale mismatch

### 8. Animation Controls (UI Panel)
**Status:** ⚠️ NOT TESTED
**Evidence:** Cannot see UI in current screenshots (zoomed out)
**Issue:** UI elements exist in HTML but need closer camera view to verify

---

## Root Cause Analysis

### Primary Issue: Mesh Vertex Count
```
Expected: ~2000-5000 vertices
Actual: 576 vertices
```

The Manifold CSG backend is producing a drastically reduced mesh. Possible causes:
1. **Union simplification** - Manifold may be merging/deduplicating vertices aggressively
2. **Object culling** - Some objects may fail to build and are silently skipped
3. **Scale mismatch** - Objects at very different scales may be problematic

### Secondary Issue: X-Ray Material Visibility
The X-ray shader uses Fresnel-based transparency:
- Colors close to white become nearly invisible
- Low-saturation colors (grays, pastels) are hard to see
- The aluminum bath (gray) may be rendering but invisible

### Tertiary Issue: Camera/Scale
- Bath is 2000mm, channels are 3mm
- Camera at 5000mm distance
- Small objects are invisible at this scale

---

## Recommendations

### Immediate Fixes
1. **Debug vertex count** - Add logging per-object to see which objects contribute 0 vertices
2. **Test without union** - Render each object individually to verify they work
3. **Increase channel size** - Make channels 10-20mm for visibility testing
4. **Use solid material** - Replace X-ray shader with solid colors for debugging

### Missing Features to Implement
1. Multi-layer tissue (skin/fat/muscle)
2. 3D gantry structure
3. Cooling manifold
4. Particle animation (already coded, needs debugging)

---

## Screenshots Taken
1. `baseline_verify.png` - Initial state
2. `view2_verify.png` - Same view, consistency check
3. `test_boxes_verify.png` - Test scene (showed old cached data)

All screenshots show same scene: pink gel strip + purple acoustic field + Z axis gizmo

---

## Files Examined
- `project/lymph_bath.lua` - Main scene file
- `renderer/src/main.ts` - Three.js renderer
- `server/src/geometry.rs` - Manifold CSG backend
- `stdlib/*.lua` - Primitives, CSG, groups, etc.

---

## Conclusion

The overnight "16 stages completed" claim is **not verified**. Core geometry rendering is broken - only ~2-3 objects render out of ~15. The acoustic field visualization partially works, but particles, gantry, cooling, and tissue layers don't exist or don't render.

**Recommendation:** Focus on fixing the mesh generation first before adding more features.
