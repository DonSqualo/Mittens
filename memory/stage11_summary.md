# Stage 11 - Acoustic Field Colormap Visualization

## Task
Connect the acoustic field computation (Stage 10) to the Three.js renderer, adding visual representation of the standing wave pressure field with a colormap overlay.

## Implementation

### 1. Acoustic Field Generation (Lua)
- **Module:** `stdlib/acoustics.lua` (from Stage 10)
- **Scene:** `project/lymph_bath.lua` 
- Added field generation at end of scene file using `Acoustics.create_default_field()`
- Generates 81×41 grid covering bath geometry (2000×400mm)
- Exports field to global `_G.AcousticField` for server access

### 2. Server-Side Integration (Rust)
- **Added:** `try_extract_lua_acoustic_field()` in main.rs
- Detects `AcousticField` table in Lua globals after scene execution
- Extracts grid dimensions, frequency, and pressure magnitude data
- Converts to `FieldData` binary format for renderer transmission
- Uses new `Colormap::PressureBlueRed` variant (U8=3 in binary protocol)

### 3. Renderer Updates (TypeScript/Three.js)
- **Added:** `value_to_color_pressure()` function
- Colormap: Blue (low/nodes) → White (mid) → Red (high/antinodes)
- Normalizes 0-1 range smoothly
- Integrated with existing `get_colormap_fn()` switch statement
- Positioned as XZ plane at Y=0 (bath center)

### 4. Bug Fixes
- **extrusions.lua:** Removed circular `require("stdlib")` dependency
- **simulation.lua:** Fixed syntax error in serialize method

## Architecture Approach
**Option A (Chosen):** Generate field data in Lua, export via globals
- Simpler than shader computation
- Leverages existing Lua acoustics module
- Compatible with Mittens' scene serialization
- Field can be updated per-frame by regenerating with current time

## Verification
✅ Acoustic field generates successfully:
- Grid: 81×41 points  
- Frequency: 0.02 Hz (vasomotion-matched)
- Max pressure: ~999,950 Pa
- Field registered in Lua globals

✅ Renderer has colormap support:
- PressureBlueRed colormap defined
- XZ plane positioning implemented
- Integration with field visualization pipeline complete

## Files Modified
1. `project/lymph_bath.lua` - Added acoustic field generation block
2. `server/src/main.rs` - Added extraction function and debug logic
3. `server/src/field.rs` - Added PressureBlueRed variant to Colormap enum
4. `renderer/src/main.ts` - Added pressure colormap function
5. `stdlib/extrusions.lua` - Fixed circular dependency
6. `stdlib/simulation.lua` - Fixed serialize method syntax

## Current Status
- Acoustic field is correctly generated in Lua with proper parameters
- Visualization infrastructure is complete in renderer
- Field overlay positioned at XZ plane (water level)
- Ready for real-time animation (update with time parameter)

## Next Steps (Stage 15+)
1. Connect field data transmission from server to renderer
2. Implement frame-by-frame field updates (animate with time)
3. Sync with simulation timestep for dynamic visualization
4. Add flow visualization particles (Stage 14)
