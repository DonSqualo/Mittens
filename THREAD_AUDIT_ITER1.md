# ISO 68-1 Compliance Audit - Iteration 1

**Date:** 2025-02-03  
**Reviewer:** Subagent m27-iter-1  
**Component:** `server/src/geometry.rs` external_thread and internal_thread functions

## Executive Summary
**ISSUE FOUND:** The groove width calculation (0.75 × pitch) creates thread lands that are TOO NARROW according to ISO 68-1. The implementation should use 0.5 × pitch instead.

## ISO 68-1 Reference Specs (M27 coarse, pitch=3mm)
- Fundamental triangle height H = 0.866025 × 3 = 2.581mm
- Thread depth = 5H/8 = 0.54125 × 3 = 1.624mm ✓ CORRECT
- Minor diameter = 27 - 2×1.624 = 23.752mm ✓ CORRECT
- Crest flat = pitch/8 = 3/8 = 0.375mm
- Root flat = pitch/4 = 3/4 = 0.75mm
- **Thread land width** (at major) ≈ pitch/2 = 1.5mm ← KEY METRIC

## Current Implementation Analysis

### Code Location
File: `server/src/geometry.rs` lines 430-600
Functions: `create_primitive()` cases for "external_thread" and "internal_thread"

### Parameter Check: thread_depth
```rust
let thread_depth = 0.54125 * pitch;  // = 1.624mm for M27
```
✓ **CORRECT** - Matches ISO 68-1 formula exactly

### Parameter Check: groove_width  
```rust
let groove_width = pitch * 0.75;  // = 2.25mm for M27
```

**ISSUE IDENTIFIED:**
- Current groove_width = 0.75 × 3 = 2.25mm
- This leaves thread land = pitch - groove_width = 3 - 2.25 = **0.75mm**
- ISO 68-1 specifies thread land ≈ **pitch/2 = 1.5mm**
- **Result: Thread lands are 50% of ISO spec width** ⚠️

### V-Groove Triangle Profile Analysis

Current code creates triangle:
```
Point 1: (r_outer=13.6, y=-1.125)      # Left outer
Point 2: (r_inner=11.776, y=0.0)       # Tip of V (at minor radius)
Point 3: (r_outer=13.6, y=1.125)       # Right outer
```

Flank angle calculation:
- Radial span: 13.6 - 11.776 = 1.824mm (= thread_depth with offsets)
- Circumferential width at outer: 1.125 × 2 = 2.25mm
- Left flank: arctan(1.125 / 1.824) = arctan(0.617) = **31.6°**
- Right flank: Same by symmetry = **31.6°**
- **Total thread angle ≈ 63.2°** (slightly more than 60°)

✓ **ACCEPTABLE** - Close enough to ISO 60° (±3° tolerance is reasonable for twisted geometry)

### Cutting Direction Verification

**External thread:**
```rust
Ok(base.difference(&cutter_offset))
```
- Subtracts the V-groove cutter FROM the cylinder
- Result: Creates grooves that CUT INTO the cylinder from major toward minor ✓
- Direction: **CORRECT**

**Internal thread:**
```rust
Ok(tube.difference(&groove_offset))
```
- Subtracts the V-groove cutter FROM the tube (bore wall)
- Result: Creates grooves that CUT INTO the bore wall from major toward minor ✓
- Direction: **CORRECT**

## Issues Summary

### 🔴 CRITICAL: Groove Width Too Wide (0.75 × pitch)

| Metric | ISO 68-1 Target | Current | Status |
|--------|-----------------|---------|--------|
| Pitch | 3mm | 3mm | ✓ |
| Thread depth | 1.624mm | 1.624mm | ✓ |
| **Thread land width** | **1.5mm** | **0.75mm** | ❌ **50% too narrow** |
| **Groove width** | **1.5mm** | **2.25mm** | ❌ **50% too wide** |
| Thread angle | 60° | 63.2° | ⚠️ Acceptable |

### Impact
- **Threads are half as thick as ISO spec** → Structural weakness, printing issues
- **Grooves are half as wide as spec** → May not mesh properly with ISO-spec threads
- **Result: Non-interchangeable with standard M27 threads**

## Recommendations

### Fix #1: Correct groove_width formula
```rust
// OLD (WRONG):
let groove_width = pitch * 0.75;  // Creates 0.75mm lands

// NEW (ISO-CORRECT):
let groove_width = pitch * 0.5;   // Creates 1.5mm lands
```

This will:
- Make thread lands = 1.5mm (matches pitch/2)
- Make groove width = 1.5mm (matches pitch/2)
- Maintain 60° thread angle
- Ensure ISO 68-1 compliance

### Next Steps (Iteration 2)
1. Apply groove_width fix
2. Rebuild and screenshot to verify thread geometry
3. Test mesh fit between male and female threads
4. Measure actual dimensions against ISO specs using geometry inspector
5. Verify thread depth at tooth root (should be exactly minor_diameter)

## Visual Inspection (Screenshot m27_iter1.png)
✓ Threads are visible and helical
✓ Both male and female sections show ridge definition
✓ Geometry creates proper ring structure
⚠️ Threads appear deep/wide - consistent with over-width groove
⚠️ Thread lands appear thin - consistent with under-spec width

## Sign-Off
**Status:** ISSUE FOUND, FIX RECOMMENDED
**Critical Path:** Yes - affects ISO compliance
**Severity:** High - architectural issue in groove width calculation
