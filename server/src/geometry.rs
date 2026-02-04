//! Manifold-based CSG geometry backend
//! Uses manifold3d for guaranteed watertight manifold meshes

use anyhow::{anyhow, Result};
use manifold3d::types::{Matrix4x3, PositiveF64, PositiveI32, Vec3};
use manifold3d::{Manifold, MeshGL};
use manifold3d_sys::{ManifoldVec2, ManifoldSimplePolygon, ManifoldPolygons, ManifoldManifold};
use mlua::{Lua, Value};
use std::alloc::{alloc, Layout};
use std::collections::HashMap;
use std::os::raw::c_void;

// Additional FFI functions for polygon extrusion
// These work around a bug in manifold3d 0.0.6 where SimplePolygon objects
// are prematurely freed in Polygons::from_simple_polygons
extern "C" {
    fn manifold_simple_polygon(
        mem: *mut c_void,
        ps: *mut ManifoldVec2,
        length: usize,
    ) -> *mut ManifoldSimplePolygon;
    
    fn manifold_polygons(
        mem: *mut c_void,
        ps: *mut *mut ManifoldSimplePolygon,
        length: usize,
    ) -> *mut ManifoldPolygons;
    
    fn manifold_extrude(
        mem: *mut c_void,
        cs: *mut ManifoldPolygons,
        height: f64,
        slices: i32,
        twist_degrees: f64,
        scale_x: f64,
        scale_y: f64,
    ) -> *mut ManifoldManifold;
    
    fn manifold_delete_simple_polygon(p: *mut ManifoldSimplePolygon);
    fn manifold_delete_polygons(p: *mut ManifoldPolygons);
    fn manifold_alloc_simple_polygon() -> *mut ManifoldSimplePolygon;
    fn manifold_alloc_polygons() -> *mut ManifoldPolygons;
}

/// Mesh data for WebSocket transfer
#[derive(Clone)]
pub struct MeshData {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub colors: Vec<f32>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn new_empty() -> Self {
        MeshData {
            positions: Vec::new(),
            normals: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn to_binary(&self) -> Vec<u8> {
        let num_vertices = (self.positions.len() / 3) as u32;
        let num_indices = self.indices.len() as u32;

        let mut data = Vec::new();
        data.extend_from_slice(&num_vertices.to_le_bytes());
        data.extend_from_slice(&num_indices.to_le_bytes());

        for &p in &self.positions {
            data.extend_from_slice(&p.to_le_bytes());
        }
        for &n in &self.normals {
            data.extend_from_slice(&n.to_le_bytes());
        }
        for &c in &self.colors {
            data.extend_from_slice(&c.to_le_bytes());
        }
        for &i in &self.indices {
            data.extend_from_slice(&i.to_le_bytes());
        }

        data
    }
}

extern "C" {
    fn manifold_meshgl_tri_verts(mem: *mut c_void, m: *mut std::ffi::c_void) -> *mut u32;
    fn manifold_alloc_meshgl() -> *mut c_void;
    fn manifold_meshgl(
        mem: *mut c_void,
        vert_props: *const f32,
        n_verts: usize,
        n_props: usize,
        tri_verts: *const u32,
        n_tris: usize,
    ) -> *mut c_void;
    fn manifold_of_meshgl(mem: *mut c_void, mesh: *mut c_void) -> *mut c_void;
    fn manifold_alloc_manifold() -> *mut c_void;
}

fn get_mesh_indices(mesh: &MeshGL, count: usize) -> Vec<u32> {
    if count == 0 {
        return vec![];
    }
    let layout = Layout::array::<u32>(count).unwrap();
    let array_ptr = unsafe { alloc(layout) } as *mut u32;

    // MeshGL stores its internal pointer at offset 0
    let mesh_ptr = unsafe { std::ptr::read(mesh as *const MeshGL as *const *mut c_void) };

    unsafe {
        manifold_meshgl_tri_verts(array_ptr as *mut c_void, mesh_ptr);
        Vec::from_raw_parts(array_ptr, count, count)
    }
}

fn manifold_to_mesh_data(manifold: &Manifold) -> MeshData {
    let mesh = manifold.as_mesh();
    let properties = mesh.vertex_properties();
    let num_props = mesh.properties_per_vertex_count() as usize;
    let num_verts = mesh.vertex_count() as usize;
    let num_tris = mesh.triangle_count() as usize;
    let index_count = num_tris * 3;

    let mut data = MeshData::new_empty();

    if num_verts == 0 || num_props < 3 {
        return data;
    }

    // Extract positions (first 3 properties per vertex)
    for i in 0..num_verts {
        let base = i * num_props;
        if base + 2 < properties.len() {
            data.positions.push(properties[base]);
            data.positions.push(properties[base + 1]);
            data.positions.push(properties[base + 2]);
        }
    }

    // Get actual triangle indices via FFI
    let indices = get_mesh_indices(&mesh, index_count);
    data.indices = indices;

    // Initialize normals
    data.normals = vec![0.0; num_verts * 3];
    let mut counts = vec![0u32; num_verts];

    // Compute normals per-face and average at vertices
    for tri in 0..num_tris {
        let base = tri * 3;
        if base + 2 >= data.indices.len() {
            continue;
        }

        let i0 = data.indices[base] as usize;
        let i1 = data.indices[base + 1] as usize;
        let i2 = data.indices[base + 2] as usize;

        if i0 >= num_verts || i1 >= num_verts || i2 >= num_verts {
            continue;
        }

        let v0 = [
            data.positions[i0 * 3],
            data.positions[i0 * 3 + 1],
            data.positions[i0 * 3 + 2],
        ];
        let v1 = [
            data.positions[i1 * 3],
            data.positions[i1 * 3 + 1],
            data.positions[i1 * 3 + 2],
        ];
        let v2 = [
            data.positions[i2 * 3],
            data.positions[i2 * 3 + 1],
            data.positions[i2 * 3 + 2],
        ];

        let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let normal = cross(edge1, edge2);

        for &idx in &[i0, i1, i2] {
            data.normals[idx * 3] += normal[0];
            data.normals[idx * 3 + 1] += normal[1];
            data.normals[idx * 3 + 2] += normal[2];
            counts[idx] += 1;
        }
    }

    // Normalize the normals
    for i in 0..num_verts {
        if counts[i] > 0 {
            let len = (data.normals[i * 3].powi(2)
                + data.normals[i * 3 + 1].powi(2)
                + data.normals[i * 3 + 2].powi(2))
            .sqrt();
            if len > 1e-10 {
                data.normals[i * 3] /= len;
                data.normals[i * 3 + 1] /= len;
                data.normals[i * 3 + 2] /= len;
            }
        }
    }

    // Default white color
    data.colors = vec![1.0; num_verts * 3];

    data
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn pos(v: f64) -> PositiveF64 {
    PositiveF64::new(v.abs().max(0.001)).unwrap()
}

fn build_manifold_primitive(obj_type: &str, params: &mlua::Table, circular_segments: u32) -> Result<Manifold> {
    match obj_type {
        "cylinder" => {
            let r: f64 = params.get("r")?;
            let h: f64 = params.get("h")?;
            // origin_at_center = false: base at z=0, extends to z=h
            Ok(Manifold::new_cylinder(
                pos(h),
                pos(r),
                None::<PositiveF64>,
                Some(manifold3d::types::PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            ))
        }
        "box" => {
            let w: f64 = params.get("w")?;
            let d: f64 = params.get::<_, f64>("d").unwrap_or(w);
            let h: f64 = params.get("h")?;
            // origin_at_center = false: corner at origin, extends to (w, d, h)
            Ok(Manifold::new_cuboid(pos(w), pos(d), pos(h), false))
        }
        "sphere" => {
            let r: f64 = params.get("r")?;
            // Spheres are always centered at origin
            Ok(Manifold::new_sphere(
                pos(r),
                None::<manifold3d::types::PositiveI32>,
            ))
        }
        "torus" => {
            let major_radius: f64 = params.get("major_radius")?;
            let minor_radius: f64 = params.get("minor_radius")?;
            let u_segments = circular_segments as usize;
            let v_segments = circular_segments as usize;
            let pi2 = 2.0 * std::f64::consts::PI;

            let num_verts = u_segments * v_segments;
            let mut vert_props: Vec<f32> = Vec::with_capacity(num_verts * 6);

            for i in 0..u_segments {
                let u = pi2 * (i as f64) / (u_segments as f64);
                let cos_u = u.cos();
                let sin_u = u.sin();
                for j in 0..v_segments {
                    let v = pi2 * (j as f64) / (v_segments as f64);
                    let cos_v = v.cos();
                    let sin_v = v.sin();
                    let x = (major_radius + minor_radius * cos_v) * cos_u;
                    let y = (major_radius + minor_radius * cos_v) * sin_u;
                    let z = minor_radius * sin_v;
                    let nx = cos_v * cos_u;
                    let ny = cos_v * sin_u;
                    let nz = sin_v;
                    vert_props.extend_from_slice(&[x as f32, y as f32, z as f32, nx as f32, ny as f32, nz as f32]);
                }
            }

            let num_tris = u_segments * v_segments * 2;
            let mut tri_verts: Vec<u32> = Vec::with_capacity(num_tris * 3);
            for i in 0..u_segments {
                let i_next = (i + 1) % u_segments;
                for j in 0..v_segments {
                    let j_next = (j + 1) % v_segments;
                    let v00 = (i * v_segments + j) as u32;
                    let v10 = (i_next * v_segments + j) as u32;
                    let v01 = (i * v_segments + j_next) as u32;
                    let v11 = (i_next * v_segments + j_next) as u32;
                    tri_verts.extend_from_slice(&[v00, v10, v11]);
                    tri_verts.extend_from_slice(&[v00, v11, v01]);
                }
            }

            let torus: Manifold = unsafe {
                let mesh_ptr = manifold_meshgl(
                    manifold_alloc_meshgl(),
                    vert_props.as_ptr(),
                    num_verts,
                    6,
                    tri_verts.as_ptr(),
                    num_tris,
                );
                let manifold_ptr = manifold_of_meshgl(manifold_alloc_manifold(), mesh_ptr);
                std::mem::transmute(manifold_ptr)
            };
            Ok(torus)
        }
        "ring" => {
            // Ring (annulus with height) for coupling coils
            // Created as difference of two cylinders
            let inner_radius: f64 = params.get("inner_radius")?;
            let outer_radius: f64 = params.get("outer_radius")?;
            let h: f64 = params.get("h")?;

            let pos = |v: f64| PositiveF64::new(v).unwrap();
            let outer = Manifold::new_cylinder(
                pos(h),
                pos(outer_radius),
                None::<PositiveF64>,
                Some(PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            );
            let inner = Manifold::new_cylinder(
                pos(h + 0.01),
                pos(inner_radius),
                None::<PositiveF64>,
                Some(PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            );

            Ok(outer.difference(&inner))
        }
        "linear_extrude" => {
            // Extrude a 2D polygon along Z axis
            // Uses direct FFI calls to work around bug in manifold3d 0.0.6
            // where SimplePolygon objects are prematurely freed
            let height: f64 = params.get("height").unwrap_or(10.0);
            
            // Collect all polygon point vectors
            let mut all_point_vecs: Vec<Vec<ManifoldVec2>> = Vec::new();
            
            // Parse outer polygon points
            let points_table: mlua::Table = params.get("points")?;
            let mut outer_points: Vec<ManifoldVec2> = Vec::new();
            for pair in points_table.pairs::<i64, mlua::Table>() {
                if let Ok((_, pt)) = pair {
                    let x: f64 = pt.get(1).or_else(|_| pt.get("x")).unwrap_or(0.0);
                    let y: f64 = pt.get(2).or_else(|_| pt.get("y")).unwrap_or(0.0);
                    outer_points.push(ManifoldVec2 { x, y });
                }
            }
            
            if outer_points.len() < 3 {
                return Err(anyhow!("linear_extrude requires at least 3 points"));
            }
            all_point_vecs.push(outer_points);
            
            // Parse holes if present
            if let Ok(holes_table) = params.get::<_, mlua::Table>("holes") {
                for pair in holes_table.pairs::<i64, mlua::Table>() {
                    if let Ok((_, hole_points_table)) = pair {
                        let mut hole_points: Vec<ManifoldVec2> = Vec::new();
                        for pt_pair in hole_points_table.pairs::<i64, mlua::Table>() {
                            if let Ok((_, pt)) = pt_pair {
                                let x: f64 = pt.get(1).or_else(|_| pt.get("x")).unwrap_or(0.0);
                                let y: f64 = pt.get(2).or_else(|_| pt.get("y")).unwrap_or(0.0);
                                hole_points.push(ManifoldVec2 { x, y });
                            }
                        }
                        if hole_points.len() >= 3 {
                            all_point_vecs.push(hole_points);
                        }
                    }
                }
            }
            
            // Create all SimplePolygon objects via FFI and keep them alive
            let mut simple_polygon_ptrs: Vec<*mut ManifoldSimplePolygon> = Vec::new();
            for mut points in all_point_vecs {
                let poly_ptr = unsafe {
                    manifold_simple_polygon(
                        manifold_alloc_simple_polygon() as *mut c_void,
                        points.as_mut_ptr(),
                        points.len(),
                    )
                };
                simple_polygon_ptrs.push(poly_ptr);
            }
            
            // Create Polygons from the SimplePolygon pointers
            let polygons_ptr = unsafe {
                manifold_polygons(
                    manifold_alloc_polygons() as *mut c_void,
                    simple_polygon_ptrs.as_mut_ptr(),
                    simple_polygon_ptrs.len(),
                )
            };
            
            // Perform extrusion
            let manifold_ptr = unsafe {
                manifold_extrude(
                    manifold_alloc_manifold() as *mut c_void,
                    polygons_ptr,
                    height,
                    1,    // slices/divisions
                    0.0,  // twist
                    1.0,  // scale_x
                    1.0,  // scale_y
                )
            };
            
            // Clean up - delete the polygons 
            // Note: We don't delete the SimplePolygons individually because
            // manifold_polygons takes ownership of them
            unsafe {
                manifold_delete_polygons(polygons_ptr);
            }
            
            // Convert raw pointer to Manifold
            let manifold: Manifold = unsafe { std::mem::transmute(manifold_ptr) };
            
            Ok(manifold)
        }
        "external_thread" => {
            // ISO metric external thread (male thread)
            // Mirror of internal_thread but pointing OUTWARD
            let major_diameter: f64 = params.get("major_diameter")?;
            let pitch: f64 = params.get("pitch").unwrap_or(3.0);
            let height: f64 = params.get("height")?;
            let segments_per_turn: usize = params.get::<_, i64>("segments_per_turn").unwrap_or(32) as usize;
            let clearance: f64 = params.get::<_, f64>("clearance").unwrap_or(0.0);
            
            // ISO 68-1 thread geometry
            let thread_depth = 0.54125 * pitch;
            let major_radius = major_diameter / 2.0 - clearance;
            let minor_radius = major_radius - thread_depth;
            
            // Create core cylinder with extension past height
            // Core radius slightly larger than minor_radius for solid union overlap
            let core_extension = pitch;
            let core_overlap = thread_depth * 0.15; // 15% overlap into thread
            let core = Manifold::new_cylinder(
                pos(height + 2.0 * core_extension),
                pos(minor_radius + core_overlap),
                None::<PositiveF64>,
                Some(PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            ).translate(Vec3::new(0.0, 0.0, -core_extension));
            
            // Generate helical thread mesh pointing OUTWARD (minor to major)
            // Same pattern as internal_thread but radii swapped
            let num_turns = height / pitch;
            let total_segments = ((num_turns + 1.0) * segments_per_turn as f64).ceil() as usize;
            let pi2 = 2.0 * std::f64::consts::PI;
            
            let half_pitch = pitch / 2.0;
            let thread_angle_factor = 0.577;
            // Apply clearance to V-profile: narrower crest for male (gentle factor)
            let crest_half_width = thread_depth * thread_angle_factor * 0.5 - clearance * 0.25;
            let root_half_width = half_pitch * 0.9;
            
            let num_profile_pts = 4usize;
            let num_verts = (total_segments + 1) * num_profile_pts;
            let mut vert_props: Vec<f32> = Vec::with_capacity(num_verts * 6);
            
            for seg in 0..=total_segments {
                let t = seg as f64 / segments_per_turn as f64;
                let angle = t * pi2;
                let z_center = t * pitch - half_pitch;
                
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                
                // Profile quad pointing OUTWARD (mirror of internal):
                // 0: inner-bottom (minor radius, z - root_half_width)
                // 1: outer-bottom (major radius, z - crest_half_width) - thread crest (narrower)
                // 2: outer-top (major radius, z + crest_half_width) - thread crest (narrower)
                // 3: inner-top (minor radius, z + root_half_width)
                
                let z0 = z_center - root_half_width;
                let z1 = z_center - crest_half_width;
                let z2 = z_center + crest_half_width;
                let z3 = z_center + root_half_width;
                
                let nx = cos_a as f32; // normal pointing outward
                let ny = sin_a as f32;
                
                vert_props.extend_from_slice(&[
                    (minor_radius * cos_a) as f32, (minor_radius * sin_a) as f32, z0 as f32,
                    nx * 0.5, ny * 0.5, -0.866,
                ]);
                vert_props.extend_from_slice(&[
                    (major_radius * cos_a) as f32, (major_radius * sin_a) as f32, z1 as f32,
                    nx, ny, 0.0,
                ]);
                vert_props.extend_from_slice(&[
                    (major_radius * cos_a) as f32, (major_radius * sin_a) as f32, z2 as f32,
                    nx, ny, 0.0,
                ]);
                vert_props.extend_from_slice(&[
                    (minor_radius * cos_a) as f32, (minor_radius * sin_a) as f32, z3 as f32,
                    nx * 0.5, ny * 0.5, 0.866,
                ]);
            }
            
            // Generate triangles - SAME winding as internal_thread
            let mut tri_verts: Vec<u32> = Vec::new();
            
            // Start cap
            tri_verts.extend_from_slice(&[0, 1, 2]);
            tri_verts.extend_from_slice(&[0, 2, 3]);
            
            // Side faces
            for seg in 0..total_segments {
                let base = (seg * num_profile_pts) as u32;
                let next = ((seg + 1) * num_profile_pts) as u32;
                
                for i in 0..num_profile_pts as u32 {
                    let next_i = (i + 1) % num_profile_pts as u32;
                    tri_verts.extend_from_slice(&[base + i, next + next_i, base + next_i]);
                    tri_verts.extend_from_slice(&[base + i, next + i, next + next_i]);
                }
            }
            
            // End cap
            let last_ring = (total_segments * num_profile_pts) as u32;
            tri_verts.extend_from_slice(&[last_ring + 0, last_ring + 2, last_ring + 1]);
            tri_verts.extend_from_slice(&[last_ring + 0, last_ring + 3, last_ring + 2]);
            
            let actual_verts = vert_props.len() / 6;
            let num_tris = tri_verts.len() / 3;
            
            let thread_mesh: Manifold = unsafe {
                let mesh_ptr = manifold_meshgl(
                    manifold_alloc_meshgl(),
                    vert_props.as_ptr(),
                    actual_verts,
                    6,
                    tri_verts.as_ptr(),
                    num_tris,
                );
                let manifold_ptr = manifold_of_meshgl(manifold_alloc_manifold(), mesh_ptr);
                std::mem::transmute(manifold_ptr)
            };
            
            // Check thread mesh
            if let Some(err) = thread_mesh.last_operation_status() {
                tracing::warn!("external_thread thread_mesh status: {:?}", err);
            } else {
                tracing::info!("external_thread thread_mesh: manifold OK");
            }
            
            // Union core with thread, then trim
            let unioned = core.union(&thread_mesh);
            
            let bound = Manifold::new_cylinder(
                pos(height),
                pos(major_radius + 0.1),
                None::<PositiveF64>,
                Some(PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            );
            
            let result = unioned.intersection(&bound);
            
            if let Some(err) = result.last_operation_status() {
                tracing::warn!("external_thread result status: {:?}", err);
            }
            
            Ok(result)
        }
        "internal_thread" => {
            // ISO metric internal thread (female thread)
            // Class 6H: reference dimension, bore defines nominal
            let major_diameter: f64 = params.get("major_diameter")?;
            let pitch: f64 = params.get("pitch").unwrap_or(3.0);
            let height: f64 = params.get("height")?;
            let segments_per_turn: usize = params.get::<_, i64>("segments_per_turn").unwrap_or(32) as usize;
            // Clearance for 3D printing - expands bore slightly for looser fit
            let clearance: f64 = params.get::<_, f64>("clearance").unwrap_or(0.0);
            
            // ISO 68-1 thread geometry
            let thread_depth = 0.54125 * pitch;
            let major_radius = major_diameter / 2.0 + clearance;  // Bore enlarged by clearance
            let minor_radius = major_radius - thread_depth;  // Crests follow
            
            // Create outer cylinder (the tube wall)
            let wall_thickness = thread_depth * 5.0; // thicker wall for stability
            let outer_radius = major_radius + wall_thickness;
            
            // Extend tube past height bounds so trim cuts through solid
            let tube_extension = pitch * 1.5; // extra extension for cleaner trim
            let outer = Manifold::new_cylinder(
                pos(height + 2.0 * tube_extension),
                pos(outer_radius),
                None::<PositiveF64>,
                Some(PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            ).translate(Vec3::new(0.0, 0.0, -tube_extension));
            
            // Create inner bore slightly smaller than MAJOR diameter for solid union overlap
            let bore_overlap = thread_depth * 0.15; // 15% overlap into thread
            let inner_bore = Manifold::new_cylinder(
                pos(height + 2.0 * tube_extension + 0.02),
                pos(major_radius - bore_overlap),
                None::<PositiveF64>,
                Some(PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            ).translate(Vec3::new(0.0, 0.0, -tube_extension - 0.01));
            
            let tube = outer.difference(&inner_bore);
            
            // Generate helical thread mesh pointing INWARD (from major to minor)
            let num_turns = height / pitch;
            let total_segments = ((num_turns + 1.0) * segments_per_turn as f64).ceil() as usize;
            let pi2 = 2.0 * std::f64::consts::PI;
            
            // Thread profile for internal: points INWARD
            let half_pitch = pitch / 2.0;
            let thread_angle_factor = 0.577;
            // Apply clearance to V-profile: wider groove for female (gentle factor)
            let crest_half_width = thread_depth * thread_angle_factor * 0.5 + clearance * 0.25;
            let root_half_width = half_pitch * 0.9;
            
            let num_profile_pts = 4usize;
            // No separate cap center vertices - caps are quad faces
            let num_verts = (total_segments + 1) * num_profile_pts;
            let mut vert_props: Vec<f32> = Vec::with_capacity(num_verts * 6);
            
            for seg in 0..=total_segments {
                let t = seg as f64 / segments_per_turn as f64;
                let angle = t * pi2;
                let z_center = t * pitch - half_pitch;
                
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                
                // Profile quad pointing INWARD:
                // 0: outer-bottom (major radius, z - root_half_width)
                // 1: inner-bottom (minor radius, z - crest_half_width) - thread crest
                // 2: inner-top (minor radius, z + crest_half_width) - thread crest
                // 3: outer-top (major radius, z + root_half_width)
                
                let z0 = z_center - root_half_width;
                let z1 = z_center - crest_half_width;
                let z2 = z_center + crest_half_width;
                let z3 = z_center + root_half_width;
                
                let nx = -(cos_a as f32); // normal pointing inward
                let ny = -(sin_a as f32);
                
                vert_props.extend_from_slice(&[
                    (major_radius * cos_a) as f32, (major_radius * sin_a) as f32, z0 as f32,
                    nx * 0.5, ny * 0.5, -0.866,
                ]);
                vert_props.extend_from_slice(&[
                    (minor_radius * cos_a) as f32, (minor_radius * sin_a) as f32, z1 as f32,
                    nx, ny, 0.0,
                ]);
                vert_props.extend_from_slice(&[
                    (minor_radius * cos_a) as f32, (minor_radius * sin_a) as f32, z2 as f32,
                    nx, ny, 0.0,
                ]);
                vert_props.extend_from_slice(&[
                    (major_radius * cos_a) as f32, (major_radius * sin_a) as f32, z3 as f32,
                    nx * 0.5, ny * 0.5, 0.866,
                ]);
            }
            
            // Generate triangles (winding for inward-facing)
            let mut tri_verts: Vec<u32> = Vec::new();
            
            // Start cap: close the first quad profile
            // Quad vertices: 0, 1, 2, 3 → triangles with reversed winding for inward
            tri_verts.extend_from_slice(&[0, 1, 2]);
            tri_verts.extend_from_slice(&[0, 2, 3]);
            
            // Side faces connecting profile rings
            for seg in 0..total_segments {
                let base = (seg * num_profile_pts) as u32;
                let next = ((seg + 1) * num_profile_pts) as u32;
                
                for i in 0..num_profile_pts as u32 {
                    let next_i = (i + 1) % num_profile_pts as u32;
                    tri_verts.extend_from_slice(&[base + i, next + next_i, base + next_i]);
                    tri_verts.extend_from_slice(&[base + i, next + i, next + next_i]);
                }
            }
            
            // End cap: close the last quad profile
            let last_ring = (total_segments * num_profile_pts) as u32;
            tri_verts.extend_from_slice(&[last_ring + 0, last_ring + 2, last_ring + 1]);
            tri_verts.extend_from_slice(&[last_ring + 0, last_ring + 3, last_ring + 2]);
            
            let actual_verts = vert_props.len() / 6;
            let num_tris = tri_verts.len() / 3;
            
            let thread_mesh: Manifold = unsafe {
                let mesh_ptr = manifold_meshgl(
                    manifold_alloc_meshgl(),
                    vert_props.as_ptr(),
                    actual_verts,
                    6,
                    tri_verts.as_ptr(),
                    num_tris,
                );
                let manifold_ptr = manifold_of_meshgl(manifold_alloc_manifold(), mesh_ptr);
                std::mem::transmute(manifold_ptr)
            };
            
            // Union tube with untrimmed thread first for better merging
            // Check if internal thread mesh is manifold
            if let Some(err) = thread_mesh.last_operation_status() {
                tracing::warn!("internal_thread thread_mesh status: {:?}", err);
            } else {
                tracing::info!("internal_thread thread_mesh: manifold OK");
            }
            
            let unioned = tube.union(&thread_mesh);
            
            // Trim to height bounds
            let bound = Manifold::new_cylinder(
                pos(height),
                pos(outer_radius + 0.1),
                None::<PositiveF64>,
                Some(PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            );
            
            let trimmed = unioned.intersection(&bound);
            
            // Clear the inner bore at minor radius (thread crests)
            // with slight overlap for clean subtraction
            let clear_bore = Manifold::new_cylinder(
                pos(height + 0.02),
                pos(minor_radius - 0.01), // Slightly smaller to ensure clean cut
                None::<PositiveF64>,
                Some(PositiveI32::new(circular_segments as i32).unwrap()),
                false,
            ).translate(Vec3::new(0.0, 0.0, -0.01));
            
            // Return without cleanup cuts - the trim bounds should be sufficient
            Ok(trimmed.difference(&clear_bore))
        }
        _ => Err(anyhow!("Unknown primitive type: {}", obj_type)),
    }
}

fn apply_manifold_ops(manifold: Manifold, table: &mlua::Table) -> Result<Manifold> {
    let mut result = manifold;

    if let Ok(ops) = table.get::<_, mlua::Table>("ops") {
        for pair in ops.pairs::<i64, mlua::Table>() {
            if let Ok((_, op_table)) = pair {
                let op: String = op_table.get("op").unwrap_or_default();
                let x: f64 = op_table.get("x").unwrap_or(0.0);
                let y: f64 = op_table.get("y").unwrap_or(0.0);
                let z: f64 = op_table.get("z").unwrap_or(0.0);

                tracing::debug!("Applying op: {} ({}, {}, {})", op, x, y, z);

                result = match op.as_str() {
                    "translate" => result.translate(Vec3::new(x, y, z)),
                    "rotate" => {
                        // Build rotation matrix from ZYX Euler angles (degrees)
                        let rx = x.to_radians();
                        let ry = y.to_radians();
                        let rz = z.to_radians();

                        let (sx, cx) = (rx.sin(), rx.cos());
                        let (sy, cy) = (ry.sin(), ry.cos());
                        let (sz, cz) = (rz.sin(), rz.cos());

                        // ZYX rotation matrix
                        let m00 = cy * cz;
                        let m01 = sx * sy * cz - cx * sz;
                        let m02 = cx * sy * cz + sx * sz;
                        let m10 = cy * sz;
                        let m11 = sx * sy * sz + cx * cz;
                        let m12 = cx * sy * sz - sx * cz;
                        let m20 = -sy;
                        let m21 = sx * cy;
                        let m22 = cx * cy;

                        let matrix = Matrix4x3::new([
                            Vec3::new(m00, m01, m02),
                            Vec3::new(m10, m11, m12),
                            Vec3::new(m20, m21, m22),
                            Vec3::new(0.0, 0.0, 0.0), // no translation
                        ]);
                        result.transform(matrix)
                    }
                    "scale" => result.scale(Vec3::new(x, y, z)),
                    _ => result,
                };
            }
        }
    }

    Ok(result)
}

fn build_manifold_object(table: &mlua::Table, circular_segments: u32) -> Result<Manifold> {
    build_manifold_object_with_components(table, circular_segments, &std::collections::HashMap::new())
}

fn build_manifold_object_with_components(
    table: &mlua::Table,
    circular_segments: u32,
    components: &std::collections::HashMap<String, mlua::Table>,
) -> Result<Manifold> {
    let obj_type: String = table.get("type")?;
    let name: String = table.get("name").unwrap_or_default();
    tracing::debug!("Building manifold object: type={}, name={}", obj_type, name);

    if obj_type == "csg" {
        let operation: String = table.get("operation")?;
        let children: mlua::Table = table.get("children")?;

        let first_child: mlua::Table = children.get(1)?;
        let mut result = build_manifold_object_with_components(&first_child, circular_segments, components)?;

        for i in 2..=children.len()? {
            let child: mlua::Table = children.get(i)?;
            let child_manifold = build_manifold_object_with_components(&child, circular_segments, components)?;
            result = match operation.as_str() {
                "union" => result.union(&child_manifold),
                "difference" => result.difference(&child_manifold),
                "intersect" => result.intersection(&child_manifold),
                _ => return Err(anyhow!("Unknown CSG operation: {}", operation)),
            };
        }

        apply_manifold_ops(result, table)
    } else if obj_type == "group" || obj_type == "assembly" {
        // Groups and assemblies are handled the same way: union of children
        let children: mlua::Table = table.get("children")?;
        let mut result: Option<Manifold> = None;

        for pair in children.pairs::<i64, mlua::Table>() {
            let (_, child) = pair?;
            let child_manifold = build_manifold_object_with_components(&child, circular_segments, components)?;
            result = Some(match result {
                Some(r) => r.union(&child_manifold),
                None => child_manifold,
            });
        }

        let manifold = result.ok_or_else(|| anyhow!("Empty group/assembly"))?;
        apply_manifold_ops(manifold, table)
    } else if obj_type == "component" {
        // Components are similar to groups but can be instanced
        let children: mlua::Table = table.get("children")?;
        let mut result: Option<Manifold> = None;

        for pair in children.pairs::<i64, mlua::Table>() {
            let (_, child) = pair?;
            let child_manifold = build_manifold_object_with_components(&child, circular_segments, components)?;
            result = Some(match result {
                Some(r) => r.union(&child_manifold),
                None => child_manifold,
            });
        }

        let manifold = result.ok_or_else(|| anyhow!("Empty component"))?;
        apply_manifold_ops(manifold, table)
    } else if obj_type == "instance" {
        // Instances reference a component by name and apply transforms
        let component_name: String = table.get("component")?;
        let component = components.get(&component_name)
            .ok_or_else(|| anyhow!("Component '{}' not found for instance", component_name))?;

        // Build the component's geometry
        let manifold = build_manifold_object_with_components(component, circular_segments, components)?;
        // Apply the instance's transforms
        apply_manifold_ops(manifold, table)
    } else {
        let params: mlua::Table = table.get("params")?;
        let manifold = build_manifold_primitive(&obj_type, &params, circular_segments)?;
        apply_manifold_ops(manifold, table)
    }
}

fn get_material_color(table: &mlua::Table) -> Option<(f32, f32, f32)> {
    // Check direct color field first
    if let Ok(color) = table.get::<_, mlua::Table>("color") {
        let r: f32 = color.get(1).unwrap_or(1.0);
        let g: f32 = color.get(2).unwrap_or(1.0);
        let b: f32 = color.get(3).unwrap_or(1.0);
        return Some((r, g, b));
    }
    // Fall back to material color
    if let Ok(material) = table.get::<_, mlua::Table>("material") {
        if let Ok(color) = material.get::<_, mlua::Table>("color") {
            let r: f32 = color.get(1).unwrap_or(1.0);
            let g: f32 = color.get(2).unwrap_or(1.0);
            let b: f32 = color.get(3).unwrap_or(1.0);
            return Some((r, g, b));
        }
    }
    None
}

fn apply_color_to_mesh(mesh: &mut MeshData, r: f32, g: f32, b: f32) {
    for i in 0..mesh.colors.len() / 3 {
        mesh.colors[i * 3] = r;
        mesh.colors[i * 3 + 1] = g;
        mesh.colors[i * 3 + 2] = b;
    }
}

fn combine_meshes(meshes: Vec<MeshData>) -> MeshData {
    let mut combined = MeshData {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::new(),
        colors: Vec::new(),
    };

    for mesh in meshes {
        let vertex_offset = (combined.positions.len() / 3) as u32;
        combined.positions.extend(&mesh.positions);
        combined.normals.extend(&mesh.normals);
        combined.colors.extend(&mesh.colors);
        combined.indices.extend(mesh.indices.iter().map(|i| i + vertex_offset));
    }

    combined
}

/// Recursively build mesh preserving per-object colors
fn build_mesh_recursive(table: &mlua::Table, circular_segments: u32) -> Result<MeshData> {
    build_mesh_recursive_with_components(table, circular_segments, &HashMap::new())
}

fn build_mesh_recursive_with_components(
    table: &mlua::Table,
    circular_segments: u32,
    components: &HashMap<String, mlua::Table>,
) -> Result<MeshData> {
    let obj_type: String = table.get("type")?;
    let obj_name: String = table.get("name").unwrap_or_default();
    let obj_tag: String = table.get("tag").unwrap_or_default();
    
    tracing::info!("Processing object: type={}, name={}, tag={}", obj_type, obj_name, obj_tag);

    if obj_type == "group" || obj_type == "assembly" || obj_type == "component" {
        // Groups, assemblies, and components all union their children
        let children: mlua::Table = table.get("children")?;
        let mut child_meshes = Vec::new();

        for pair in children.pairs::<i64, mlua::Table>() {
            let (_, child) = pair?;
            let child_mesh = build_mesh_recursive_with_components(&child, circular_segments, components)?;
            tracing::info!("  Child mesh: {} vertices, {} triangles", 
                child_mesh.positions.len() / 3, 
                child_mesh.indices.len() / 3);
            child_meshes.push(child_mesh);
        }

        let mut combined = if child_meshes.is_empty() {
            return Err(anyhow!("Empty group/assembly/component"));
        } else {
            combine_meshes(child_meshes)
        };
        
        tracing::info!("Combined group '{}': {} vertices, {} triangles", 
            obj_name, combined.positions.len() / 3, combined.indices.len() / 3);

        // Apply group-level material if present (overrides children)
        if let Some((r, g, b)) = get_material_color(table) {
            apply_color_to_mesh(&mut combined, r, g, b);
        }

        // Apply group-level transforms
        if let Ok(ops) = table.get::<_, mlua::Table>("ops") {
            apply_mesh_transforms(&mut combined, &ops)?;
        }

        Ok(combined)
    } else if obj_type == "instance" {
        // Instances reference a component by name and apply transforms
        let component_name: String = table.get("component")?;
        let component = components.get(&component_name)
            .ok_or_else(|| anyhow!("Component '{}' not found for instance", component_name))?;

        // Build the component's geometry
        let mut mesh = build_mesh_recursive_with_components(component, circular_segments, components)?;

        // Apply the instance's transforms
        if let Ok(ops) = table.get::<_, mlua::Table>("ops") {
            apply_mesh_transforms(&mut mesh, &ops)?;
        }

        Ok(mesh)
    } else if obj_type == "csg" {
        // For CSG, we need to use Manifold for correct boolean operations
        let manifold = build_manifold_object(table, circular_segments)?;
        let mut mesh = manifold_to_mesh_data(&manifold);

        // Try to get color from result, then from first child
        if let Some((r, g, b)) = get_material_color(table) {
            apply_color_to_mesh(&mut mesh, r, g, b);
        } else if let Ok(children) = table.get::<_, mlua::Table>("children") {
            if let Ok(first_child) = children.get::<_, mlua::Table>(1) {
                if let Some((r, g, b)) = get_material_color(&first_child) {
                    apply_color_to_mesh(&mut mesh, r, g, b);
                }
            }
        }

        Ok(mesh)
    } else {
        // Primitive
        let params: mlua::Table = table.get("params")?;
        let manifold = build_manifold_primitive(&obj_type, &params, circular_segments)?;
        let manifold = apply_manifold_ops(manifold, table)?;
        let mut mesh = manifold_to_mesh_data(&manifold);

        if let Some((r, g, b)) = get_material_color(table) {
            apply_color_to_mesh(&mut mesh, r, g, b);
        }

        Ok(mesh)
    }
}

fn apply_mesh_transforms(mesh: &mut MeshData, ops: &mlua::Table) -> Result<()> {
    for pair in ops.clone().pairs::<i64, mlua::Table>() {
        if let Ok((_, op_table)) = pair {
            let op: String = op_table.get("op").unwrap_or_default();
            let x: f64 = op_table.get("x").unwrap_or(0.0);
            let y: f64 = op_table.get("y").unwrap_or(0.0);
            let z: f64 = op_table.get("z").unwrap_or(0.0);

            match op.as_str() {
                "translate" => {
                    for i in 0..mesh.positions.len() / 3 {
                        mesh.positions[i * 3] += x as f32;
                        mesh.positions[i * 3 + 1] += y as f32;
                        mesh.positions[i * 3 + 2] += z as f32;
                    }
                }
                "rotate" => {
                    let rx = x.to_radians();
                    let ry = y.to_radians();
                    let rz = z.to_radians();

                    let (sx, cx) = (rx.sin() as f32, rx.cos() as f32);
                    let (sy, cy) = (ry.sin() as f32, ry.cos() as f32);
                    let (sz, cz) = (rz.sin() as f32, rz.cos() as f32);

                    let m00 = cy * cz;
                    let m01 = sx * sy * cz - cx * sz;
                    let m02 = cx * sy * cz + sx * sz;
                    let m10 = cy * sz;
                    let m11 = sx * sy * sz + cx * cz;
                    let m12 = cx * sy * sz - sx * cz;
                    let m20 = -sy;
                    let m21 = sx * cy;
                    let m22 = cx * cy;

                    for i in 0..mesh.positions.len() / 3 {
                        let px = mesh.positions[i * 3];
                        let py = mesh.positions[i * 3 + 1];
                        let pz = mesh.positions[i * 3 + 2];

                        mesh.positions[i * 3] = m00 * px + m01 * py + m02 * pz;
                        mesh.positions[i * 3 + 1] = m10 * px + m11 * py + m12 * pz;
                        mesh.positions[i * 3 + 2] = m20 * px + m21 * py + m22 * pz;

                        let nx = mesh.normals[i * 3];
                        let ny = mesh.normals[i * 3 + 1];
                        let nz = mesh.normals[i * 3 + 2];

                        mesh.normals[i * 3] = m00 * nx + m01 * ny + m02 * nz;
                        mesh.normals[i * 3 + 1] = m10 * nx + m11 * ny + m12 * nz;
                        mesh.normals[i * 3 + 2] = m20 * nx + m21 * ny + m22 * nz;
                    }
                }
                "scale" => {
                    // Scale positions
                    for i in 0..mesh.positions.len() / 3 {
                        mesh.positions[i * 3] *= x as f32;
                        mesh.positions[i * 3 + 1] *= y as f32;
                        mesh.positions[i * 3 + 2] *= z as f32;
                    }
                    // For non-uniform scaling, normals must use transpose of inverse
                    // The inverse of scale(x,y,z) is scale(1/x, 1/y, 1/z)
                    // Transpose of diagonal matrix is itself
                    let inv_x = if x.abs() > 1e-10 { 1.0 / x } else { 1.0 };
                    let inv_y = if y.abs() > 1e-10 { 1.0 / y } else { 1.0 };
                    let inv_z = if z.abs() > 1e-10 { 1.0 / z } else { 1.0 };
                    for i in 0..mesh.normals.len() / 3 {
                        mesh.normals[i * 3] *= inv_x as f32;
                        mesh.normals[i * 3 + 1] *= inv_y as f32;
                        mesh.normals[i * 3 + 2] *= inv_z as f32;
                        // Re-normalize
                        let nx = mesh.normals[i * 3];
                        let ny = mesh.normals[i * 3 + 1];
                        let nz = mesh.normals[i * 3 + 2];
                        let len = (nx * nx + ny * ny + nz * nz).sqrt();
                        if len > 1e-10 {
                            mesh.normals[i * 3] /= len;
                            mesh.normals[i * 3 + 1] /= len;
                            mesh.normals[i * 3 + 2] /= len;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Build mesh from a serialized object using Manifold
pub fn build_object_manifold(table: &mlua::Table, circular_segments: u32) -> Result<MeshData> {
    build_mesh_recursive(table, circular_segments)
}

/// Build mesh from a serialized object using Manifold, with optional degenerate triangle removal
pub fn build_object_manifold_clean(table: &mlua::Table, circular_segments: u32, remove_degenerates: bool) -> Result<MeshData> {
    let mut mesh = build_mesh_recursive(table, circular_segments)?;
    if remove_degenerates {
        let removed = remove_degenerate_triangles(&mut mesh);
        if removed > 0 {
            tracing::debug!("Removed {} degenerate triangles", removed);
        }
    }
    Ok(mesh)
}

/// Generate mesh from Lua scene using Manifold backend
pub fn generate_mesh_from_lua_manifold(_lua: &Lua, value: &Value, circular_segments: u32) -> Result<MeshData> {
    generate_mesh_from_lua_manifold_clean(_lua, value, circular_segments, false)
}

/// Generate mesh from Lua scene using Manifold backend, with optional degenerate triangle removal
pub fn generate_mesh_from_lua_manifold_clean(_lua: &Lua, value: &Value, circular_segments: u32, remove_degenerates: bool) -> Result<MeshData> {
    let table = value.as_table().ok_or_else(|| anyhow!("Expected table"))?;
    let objects: mlua::Table = table.get("objects")?;

    let mut meshes = Vec::new();

    for pair in objects.pairs::<i64, mlua::Table>() {
        let (_, obj) = pair?;
        let mesh = build_mesh_recursive(&obj, circular_segments)?;
        meshes.push(mesh);
    }

    if meshes.is_empty() {
        return Err(anyhow!("No objects in scene"));
    }

    let mut combined = combine_meshes(meshes);
    if remove_degenerates {
        let removed = remove_degenerate_triangles(&mut combined);
        if removed > 0 {
            tracing::debug!("Removed {} degenerate triangles from scene", removed);
        }
    }
    Ok(combined)
}

/// Generate mesh from a single serialized object using Manifold
pub fn generate_mesh_from_object_manifold(_lua: &Lua, table: &mlua::Table, circular_segments: u32) -> Result<MeshData> {
    build_object_manifold(table, circular_segments)
}

/// Generate mesh from a single serialized object using Manifold, with optional degenerate triangle removal
pub fn generate_mesh_from_object_manifold_clean(_lua: &Lua, table: &mlua::Table, circular_segments: u32, remove_degenerates: bool) -> Result<MeshData> {
    build_object_manifold_clean(table, circular_segments, remove_degenerates)
}

// ===========================
// Mesh Validation and Cleanup

/// Remove degenerate triangles (zero area) from a mesh
/// Returns the count of removed triangles
pub fn remove_degenerate_triangles(mesh: &mut MeshData) -> usize {
    let num_vertices = mesh.positions.len() / 3;
    let num_tris = mesh.indices.len() / 3;
    let mut valid_indices = Vec::with_capacity(mesh.indices.len());
    let mut removed = 0;

    for tri in 0..num_tris {
        let base = tri * 3;
        let i0 = mesh.indices[base] as usize;
        let i1 = mesh.indices[base + 1] as usize;
        let i2 = mesh.indices[base + 2] as usize;

        // Skip if any index is out of bounds
        if i0 >= num_vertices || i1 >= num_vertices || i2 >= num_vertices {
            removed += 1;
            continue;
        }

        let v0 = [
            mesh.positions[i0 * 3],
            mesh.positions[i0 * 3 + 1],
            mesh.positions[i0 * 3 + 2],
        ];
        let v1 = [
            mesh.positions[i1 * 3],
            mesh.positions[i1 * 3 + 1],
            mesh.positions[i1 * 3 + 2],
        ];
        let v2 = [
            mesh.positions[i2 * 3],
            mesh.positions[i2 * 3 + 1],
            mesh.positions[i2 * 3 + 2],
        ];

        let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let normal = cross(edge1, edge2);
        let area_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];

        if area_sq < 1e-12 {
            removed += 1;
        } else {
            valid_indices.push(mesh.indices[base]);
            valid_indices.push(mesh.indices[base + 1]);
            valid_indices.push(mesh.indices[base + 2]);
        }
    }

    mesh.indices = valid_indices;
    removed
}

/// Validation result with warnings
pub struct MeshValidation {
    pub valid: bool,
    pub warnings: Vec<String>,
}

/// Validate mesh for common issues
pub fn validate_mesh(mesh: &MeshData) -> MeshValidation {
    let mut warnings = Vec::new();
    let mut valid = true;

    // Check for NaN/Inf in positions
    for (i, &p) in mesh.positions.iter().enumerate() {
        if !p.is_finite() {
            warnings.push(format!("Position {} has non-finite value: {}", i / 3, p));
            valid = false;
        }
    }

    // Check for NaN/Inf in normals
    for (i, &n) in mesh.normals.iter().enumerate() {
        if !n.is_finite() {
            warnings.push(format!("Normal {} has non-finite value: {}", i / 3, n));
            valid = false;
        }
    }

    // Check for valid indices
    let num_vertices = mesh.positions.len() / 3;
    for (i, &idx) in mesh.indices.iter().enumerate() {
        if idx as usize >= num_vertices {
            warnings.push(format!("Index {} references out-of-bounds vertex {}", i, idx));
            valid = false;
        }
    }

    // Check for degenerate triangles (zero area)
    let num_tris = mesh.indices.len() / 3;
    let mut degenerate_count = 0;
    for tri in 0..num_tris {
        let base = tri * 3;
        if base + 2 >= mesh.indices.len() {
            continue;
        }
        let i0 = mesh.indices[base] as usize;
        let i1 = mesh.indices[base + 1] as usize;
        let i2 = mesh.indices[base + 2] as usize;

        if i0 >= num_vertices || i1 >= num_vertices || i2 >= num_vertices {
            continue;
        }

        let v0 = [
            mesh.positions[i0 * 3],
            mesh.positions[i0 * 3 + 1],
            mesh.positions[i0 * 3 + 2],
        ];
        let v1 = [
            mesh.positions[i1 * 3],
            mesh.positions[i1 * 3 + 1],
            mesh.positions[i1 * 3 + 2],
        ];
        let v2 = [
            mesh.positions[i2 * 3],
            mesh.positions[i2 * 3 + 1],
            mesh.positions[i2 * 3 + 2],
        ];

        let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let normal = cross(edge1, edge2);
        let area_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];

        if area_sq < 1e-12 {
            degenerate_count += 1;
        }
    }

    if degenerate_count > 0 {
        warnings.push(format!("{} degenerate triangles (zero area)", degenerate_count));
    }

    // Check mesh bounds (warn if very small or very large)
    if num_vertices > 0 {
        let (mut min_x, mut min_y, mut min_z) = (f32::MAX, f32::MAX, f32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (f32::MIN, f32::MIN, f32::MIN);

        for i in 0..num_vertices {
            let x = mesh.positions[i * 3];
            let y = mesh.positions[i * 3 + 1];
            let z = mesh.positions[i * 3 + 2];
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            min_z = min_z.min(z);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            max_z = max_z.max(z);
        }

        let size_x = max_x - min_x;
        let size_y = max_y - min_y;
        let size_z = max_z - min_z;

        if size_x < 1e-6 || size_y < 1e-6 || size_z < 1e-6 {
            warnings.push(format!(
                "Mesh has near-zero extent: ({:.6}, {:.6}, {:.6})",
                size_x, size_y, size_z
            ));
        }

        if size_x > 1e6 || size_y > 1e6 || size_z > 1e6 {
            warnings.push(format!(
                "Mesh has extremely large extent: ({:.1}, {:.1}, {:.1}) - check units",
                size_x, size_y, size_z
            ));
        }
    }

    MeshValidation { valid, warnings }
}

// ===========================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_mesh_valid() {
        // Simple triangle with sufficient extent in all 3 dimensions
        let mesh = MeshData {
            positions: vec![0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 5.0, 10.0, 10.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            colors: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2],
        };
        let result = validate_mesh(&mesh);
        assert!(result.valid, "Valid mesh should pass validation");
        assert!(result.warnings.is_empty(), "Valid mesh should have no warnings: {:?}", result.warnings);
    }

    #[test]
    fn test_validate_mesh_degenerate_triangle() {
        let mesh = MeshData {
            positions: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], // degenerate
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            colors: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2],
        };
        let result = validate_mesh(&mesh);
        assert!(result.warnings.iter().any(|w| w.contains("degenerate")));
    }

    #[test]
    fn test_validate_mesh_nan_position() {
        let mesh = MeshData {
            positions: vec![f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 1.0, 0.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            colors: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2],
        };
        let result = validate_mesh(&mesh);
        assert!(!result.valid, "Mesh with NaN should fail validation");
    }

    #[test]
    fn test_validate_mesh_out_of_bounds_index() {
        let mesh = MeshData {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 1.0, 0.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            colors: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 99], // Out of bounds
        };
        let result = validate_mesh(&mesh);
        assert!(!result.valid, "Mesh with out-of-bounds index should fail");
    }

    #[test]
    fn test_remove_degenerate_triangles() {
        // Create mesh with 3 triangles: 1 valid, 2 degenerate
        // 6 vertices total
        let mut mesh = MeshData {
            positions: vec![
                // Valid triangle vertices (0, 1, 2)
                0.0, 0.0, 0.0,
                10.0, 0.0, 0.0,
                5.0, 10.0, 10.0,
                // Degenerate triangle 1: all same point (3, 4, 5)
                1.0, 1.0, 1.0,
                1.0, 1.0, 1.0,
                1.0, 1.0, 1.0,
            ],
            normals: vec![
                0.0, 0.0, 1.0,
                0.0, 0.0, 1.0,
                0.0, 0.0, 1.0,
                0.0, 0.0, 1.0,
                0.0, 0.0, 1.0,
                0.0, 0.0, 1.0,
            ],
            colors: vec![
                1.0, 1.0, 1.0,
                1.0, 1.0, 1.0,
                1.0, 1.0, 1.0,
                1.0, 1.0, 1.0,
                1.0, 1.0, 1.0,
                1.0, 1.0, 1.0,
            ],
            // Triangle 1: valid (0,1,2), Triangle 2: degenerate (3,4,5), Triangle 3: collinear (0,0,1)
            indices: vec![0, 1, 2, 3, 4, 5, 0, 0, 1],
        };

        let original_tri_count = mesh.indices.len() / 3;
        assert_eq!(original_tri_count, 3);

        let removed = remove_degenerate_triangles(&mut mesh);

        assert_eq!(removed, 2, "Should remove 2 degenerate triangles");
        assert_eq!(mesh.indices.len(), 3, "Should have 3 indices left (1 triangle)");
        assert_eq!(mesh.indices, vec![0, 1, 2], "Valid triangle should remain");
    }

    #[test]
    fn test_remove_degenerate_triangles_all_valid() {
        let mut mesh = MeshData {
            positions: vec![
                0.0, 0.0, 0.0,
                10.0, 0.0, 0.0,
                5.0, 10.0, 10.0,
            ],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            colors: vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            indices: vec![0, 1, 2],
        };

        let removed = remove_degenerate_triangles(&mut mesh);

        assert_eq!(removed, 0, "No triangles should be removed");
        assert_eq!(mesh.indices.len(), 3, "Should still have 3 indices");
    }

    #[test]
    fn test_remove_degenerate_triangles_out_of_bounds() {
        let mut mesh = MeshData {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 1.0, 1.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            colors: vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            indices: vec![0, 1, 2, 0, 1, 99], // Second triangle has out-of-bounds index
        };

        let removed = remove_degenerate_triangles(&mut mesh);

        assert_eq!(removed, 1, "Out-of-bounds triangle should be removed");
        assert_eq!(mesh.indices.len(), 3, "Should have 3 indices left");
        assert_eq!(mesh.indices, vec![0, 1, 2]);
    }
}
