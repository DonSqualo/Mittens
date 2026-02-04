// Single-surface swept thread primitives
// No boolean operations - generates watertight meshes directly

use manifold3d::types::{PositiveF64, PositiveI32};
use manifold3d::Manifold;

/// Thread profile shape - maps position within one pitch cycle [0, 1) to radius offset [0, 1]
/// ISO metric 60° thread with flat crest and root
fn thread_profile(t: f64) -> f64 {
    // t is position within one pitch cycle [0, 1)
    // Returns 0 at root, 1 at crest
    // ISO thread: 60° angle, with flat crest (1/8 pitch) and flat root (1/4 pitch)
    
    let t = t.rem_euclid(1.0);
    
    // Profile within one tooth:
    // 0.0 - 0.125: flat root
    // 0.125 - 0.375: rising flank  
    // 0.375 - 0.625: flat crest
    // 0.625 - 0.875: falling flank
    // 0.875 - 1.0: flat root (continues to next tooth)
    
    if t < 0.125 || t >= 0.875 {
        0.0 // flat root
    } else if t < 0.375 {
        // rising flank: linear from 0 to 1
        (t - 0.125) / 0.25
    } else if t < 0.625 {
        1.0 // flat crest
    } else {
        // falling flank: linear from 1 to 0
        1.0 - (t - 0.625) / 0.25
    }
}

/// Generate external (male) thread as single watertight mesh
/// Returns vertices (x,y,z,nx,ny,nz) and triangle indices
pub fn generate_external_thread(
    major_diameter: f64,
    pitch: f64,
    height: f64,
    segments_per_turn: usize,
    clearance: f64,
) -> (Vec<f32>, Vec<u32>) {
    let pi2 = std::f64::consts::PI * 2.0;
    
    // ISO 68-1 thread geometry
    let thread_depth = 0.54125 * pitch;
    let major_radius = major_diameter / 2.0 - clearance;
    let minor_radius = major_radius - thread_depth;
    
    // Number of z-slices: enough to capture thread detail
    let num_turns = height / pitch;
    let z_segments = ((num_turns + 0.5) * segments_per_turn as f64).ceil() as usize;
    let angle_segments = segments_per_turn;
    
    // Total vertices: (z_segments + 1) rings × angle_segments vertices + 2 center vertices for caps
    let ring_verts = angle_segments;
    let total_rings = z_segments + 1;
    let num_verts = total_rings * ring_verts + 2; // +2 for cap centers
    
    let mut vert_props: Vec<f32> = Vec::with_capacity(num_verts * 6);
    
    // Generate vertex rings
    for zi in 0..=z_segments {
        let z = (zi as f64 / z_segments as f64) * height;
        let z_phase = z / pitch; // how many turns at this z
        
        for ai in 0..angle_segments {
            let angle = (ai as f64 / angle_segments as f64) * pi2;
            
            // Thread phase: combination of angle and z
            // As z increases, the thread rotates
            let phase = (angle / pi2 - z_phase).rem_euclid(1.0);
            let profile_val = thread_profile(phase);
            
            // Radius varies from minor to major based on profile
            let r = minor_radius + thread_depth * profile_val;
            
            let x = r * angle.cos();
            let y = r * angle.sin();
            
            // Normal: approximate by profile gradient
            let phase_next = ((angle / pi2 + 0.01) - z_phase).rem_euclid(1.0);
            let profile_next = thread_profile(phase_next);
            let dr_dangle = (profile_next - profile_val) * thread_depth / 0.01;
            
            // Tangent in angle direction
            let tx = -r * angle.sin() + dr_dangle * angle.cos();
            let ty = r * angle.cos() + dr_dangle * angle.sin();
            
            // Tangent in z direction (thread slope)
            let dz_per_turn = pitch;
            let dr_dz = -thread_depth * (profile_next - profile_val) / (dz_per_turn / angle_segments as f64);
            let tz_x = dr_dz * angle.cos();
            let tz_y = dr_dz * angle.sin();
            let tz_z = 1.0;
            
            // Normal = cross product of tangents (pointing outward)
            let nx = ty * tz_z - 0.0 * tz_y;
            let ny = 0.0 * tz_x - tx * tz_z;
            let nz = tx * tz_y - ty * tz_x;
            let len = (nx*nx + ny*ny + nz*nz).sqrt().max(0.001);
            
            vert_props.extend_from_slice(&[
                x as f32, y as f32, z as f32,
                (nx / len) as f32, (ny / len) as f32, (nz / len) as f32,
            ]);
        }
    }
    
    // Bottom cap center vertex
    let bottom_center_idx = (total_rings * ring_verts) as u32;
    vert_props.extend_from_slice(&[0.0, 0.0, 0.0, 0.0, 0.0, -1.0]);
    
    // Top cap center vertex  
    let top_center_idx = bottom_center_idx + 1;
    vert_props.extend_from_slice(&[0.0, 0.0, height as f32, 0.0, 0.0, 1.0]);
    
    // Generate triangles
    let mut tri_verts: Vec<u32> = Vec::new();
    
    // Side faces: connect adjacent rings
    for zi in 0..z_segments {
        let ring_base = (zi * ring_verts) as u32;
        let ring_next = ((zi + 1) * ring_verts) as u32;
        
        for ai in 0..angle_segments {
            let a0 = ai as u32;
            let a1 = ((ai + 1) % angle_segments) as u32;
            
            // Quad as two triangles (CCW winding for outward normal)
            tri_verts.extend_from_slice(&[
                ring_base + a0,
                ring_base + a1,
                ring_next + a1,
            ]);
            tri_verts.extend_from_slice(&[
                ring_base + a0,
                ring_next + a1,
                ring_next + a0,
            ]);
        }
    }
    
    // Bottom cap: fan from center to first ring (CW for downward normal)
    for ai in 0..angle_segments {
        let a0 = ai as u32;
        let a1 = ((ai + 1) % angle_segments) as u32;
        tri_verts.extend_from_slice(&[bottom_center_idx, a1, a0]);
    }
    
    // Top cap: fan from center to last ring (CCW for upward normal)
    let last_ring_base = (z_segments * ring_verts) as u32;
    for ai in 0..angle_segments {
        let a0 = ai as u32;
        let a1 = ((ai + 1) % angle_segments) as u32;
        tri_verts.extend_from_slice(&[top_center_idx, last_ring_base + a0, last_ring_base + a1]);
    }
    
    (vert_props, tri_verts)
}

/// Generate internal (female) thread as single watertight mesh
/// This creates a tube with threaded inner surface
pub fn generate_internal_thread(
    major_diameter: f64,
    pitch: f64,
    height: f64,
    segments_per_turn: usize,
    clearance: f64,
    wall_thickness: f64,
) -> (Vec<f32>, Vec<u32>) {
    let pi2 = std::f64::consts::PI * 2.0;
    
    // ISO 68-1 thread geometry  
    let thread_depth = 0.54125 * pitch;
    let major_radius = major_diameter / 2.0 + clearance; // Bore enlarged
    let minor_radius = major_radius - thread_depth;
    let outer_radius = major_radius + wall_thickness;
    
    // Number of z-slices
    let num_turns = height / pitch;
    let z_segments = ((num_turns + 0.5) * segments_per_turn as f64).ceil() as usize;
    let angle_segments = segments_per_turn;
    
    // Vertices: inner ring + outer ring at each z level, plus cap vertices
    let ring_verts = angle_segments * 2; // inner and outer
    let total_rings = z_segments + 1;
    let num_verts = total_rings * ring_verts;
    
    let mut vert_props: Vec<f32> = Vec::with_capacity(num_verts * 6);
    
    // Generate vertex rings (inner then outer at each z)
    for zi in 0..=z_segments {
        let z = (zi as f64 / z_segments as f64) * height;
        let z_phase = z / pitch;
        
        // Inner ring (threaded surface)
        for ai in 0..angle_segments {
            let angle = (ai as f64 / angle_segments as f64) * pi2;
            
            // Thread phase
            let phase = (angle / pi2 - z_phase).rem_euclid(1.0);
            let profile_val = thread_profile(phase);
            
            // For internal thread: radius is LARGEST at root, SMALLEST at crest
            // (opposite of external - the thread sticks inward)
            let r = major_radius - thread_depth * profile_val;
            
            let x = r * angle.cos();
            let y = r * angle.sin();
            
            // Normal pointing inward (negative radial)
            let nx = -angle.cos();
            let ny = -angle.sin();
            
            vert_props.extend_from_slice(&[
                x as f32, y as f32, z as f32,
                nx as f32, ny as f32, 0.0,
            ]);
        }
        
        // Outer ring (smooth cylinder)
        for ai in 0..angle_segments {
            let angle = (ai as f64 / angle_segments as f64) * pi2;
            
            let x = outer_radius * angle.cos();
            let y = outer_radius * angle.sin();
            
            // Normal pointing outward
            let nx = angle.cos();
            let ny = angle.sin();
            
            vert_props.extend_from_slice(&[
                x as f32, y as f32, z as f32,
                nx as f32, ny as f32, 0.0,
            ]);
        }
    }
    
    let mut tri_verts: Vec<u32> = Vec::new();
    
    // Inner surface faces (winding for inward-facing)
    for zi in 0..z_segments {
        let ring_base = (zi * ring_verts) as u32;
        let ring_next = ((zi + 1) * ring_verts) as u32;
        
        for ai in 0..angle_segments {
            let a0 = ai as u32;
            let a1 = ((ai + 1) % angle_segments) as u32;
            
            // Inner surface: CW winding for inward normal
            tri_verts.extend_from_slice(&[
                ring_base + a0,
                ring_next + a0,
                ring_next + a1,
            ]);
            tri_verts.extend_from_slice(&[
                ring_base + a0,
                ring_next + a1,
                ring_base + a1,
            ]);
        }
    }
    
    // Outer surface faces (winding for outward-facing)
    let outer_offset = angle_segments as u32;
    for zi in 0..z_segments {
        let ring_base = (zi * ring_verts) as u32 + outer_offset;
        let ring_next = ((zi + 1) * ring_verts) as u32 + outer_offset;
        
        for ai in 0..angle_segments {
            let a0 = ai as u32;
            let a1 = ((ai + 1) % angle_segments) as u32;
            
            // Outer surface: CCW winding for outward normal
            tri_verts.extend_from_slice(&[
                ring_base + a0,
                ring_base + a1,
                ring_next + a1,
            ]);
            tri_verts.extend_from_slice(&[
                ring_base + a0,
                ring_next + a1,
                ring_next + a0,
            ]);
        }
    }
    
    // Bottom cap: annulus connecting inner and outer at z=0
    // Inner surface bottom edge goes: a1 → a0 (from CW winding)
    // Outer surface bottom edge goes: a0 → a1 (from CCW winding)
    // Cap must provide: inner a0 → a1, outer a1 → a0
    let bottom_inner = 0u32;
    let bottom_outer = outer_offset;
    for ai in 0..angle_segments {
        let a0 = ai as u32;
        let a1 = ((ai + 1) % angle_segments) as u32;
        
        // Quad connecting inner to outer (normal points -Z)
        // inner_a0 → inner_a1 → outer_a1 → outer_a0
        tri_verts.extend_from_slice(&[
            bottom_inner + a0,
            bottom_inner + a1,
            bottom_outer + a1,
        ]);
        tri_verts.extend_from_slice(&[
            bottom_inner + a0,
            bottom_outer + a1,
            bottom_outer + a0,
        ]);
    }
    
    // Top cap: annulus connecting inner and outer at z=height
    // Inner surface top edge goes: a0 → a1 (from CW winding)
    // Outer surface top edge goes: a1 → a0 (from CCW winding)
    // Cap must provide: inner a1 → a0, outer a0 → a1
    let top_ring_base = (z_segments * ring_verts) as u32;
    let top_inner = top_ring_base;
    let top_outer = top_ring_base + outer_offset;
    for ai in 0..angle_segments {
        let a0 = ai as u32;
        let a1 = ((ai + 1) % angle_segments) as u32;
        
        // Quad connecting inner to outer (normal points +Z)
        // inner_a1 → inner_a0 → outer_a0 → outer_a1
        tri_verts.extend_from_slice(&[
            top_inner + a1,
            top_inner + a0,
            top_outer + a0,
        ]);
        tri_verts.extend_from_slice(&[
            top_inner + a1,
            top_outer + a0,
            top_outer + a1,
        ]);
    }
    
    (vert_props, tri_verts)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_thread_profile() {
        // Root regions
        assert!((thread_profile(0.0) - 0.0).abs() < 0.01);
        assert!((thread_profile(0.9) - 0.0).abs() < 0.01);
        
        // Crest region
        assert!((thread_profile(0.5) - 1.0).abs() < 0.01);
        
        // Mid-flank
        assert!((thread_profile(0.25) - 0.5).abs() < 0.01);
        assert!((thread_profile(0.75) - 0.5).abs() < 0.01);
    }
    
    #[test]
    fn test_external_thread_generation() {
        let (verts, tris) = generate_external_thread(27.0, 3.0, 10.0, 32, 0.0);
        
        // Basic sanity checks
        assert!(verts.len() > 0);
        assert!(tris.len() > 0);
        assert_eq!(verts.len() % 6, 0); // 6 floats per vertex
        assert_eq!(tris.len() % 3, 0);  // 3 indices per triangle
        
        // Check all triangle indices are valid
        let num_verts = verts.len() / 6;
        for idx in &tris {
            assert!((*idx as usize) < num_verts, "Invalid vertex index: {} >= {}", idx, num_verts);
        }
    }
    
    #[test]
    fn test_internal_thread_generation() {
        let (verts, tris) = generate_internal_thread(27.0, 3.0, 10.0, 32, 0.0, 5.0);
        
        assert!(verts.len() > 0);
        assert!(tris.len() > 0);
        assert_eq!(verts.len() % 6, 0);
        assert_eq!(tris.len() % 3, 0);
        
        let num_verts = verts.len() / 6;
        for idx in &tris {
            assert!((*idx as usize) < num_verts, "Invalid vertex index: {} >= {}", idx, num_verts);
        }
    }
}
