//! Manifold-based CSG geometry backend
//! Uses manifold3d for guaranteed watertight manifold meshes

use anyhow::{anyhow, Result};
use manifold3d::types::{Matrix4x3, PositiveF64, PositiveI32, Vec3};
use manifold3d::{Manifold, MeshGL};
use mlua::{Lua, Value};
use std::alloc::{alloc, Layout};
use std::collections::HashMap;
use std::os::raw::c_void;

/// Mesh data for WebSocket transfer
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

    /// Generate blueprint edges projected onto a plane
    /// plane: 0=XZ, 1=XY, 2=YZ
    pub fn generate_blueprint(&self, plane: u8) -> Vec<u8> {
        use std::collections::HashMap;
        
        // Crease angle threshold (cosine of ~30 degrees = 0.866)
        // Edges where adjacent faces differ by more than this are "feature edges"
        const CREASE_THRESHOLD: f32 = 0.5; // ~60 degrees
        
        // Compute face normals for each triangle
        let num_tris = self.indices.len() / 3;
        let mut face_normals: Vec<[f32; 3]> = Vec::with_capacity(num_tris);
        
        for tri in 0..num_tris {
            let i0 = self.indices[tri * 3] as usize;
            let i1 = self.indices[tri * 3 + 1] as usize;
            let i2 = self.indices[tri * 3 + 2] as usize;
            
            let p0 = [self.positions[i0 * 3], self.positions[i0 * 3 + 1], self.positions[i0 * 3 + 2]];
            let p1 = [self.positions[i1 * 3], self.positions[i1 * 3 + 1], self.positions[i1 * 3 + 2]];
            let p2 = [self.positions[i2 * 3], self.positions[i2 * 3 + 1], self.positions[i2 * 3 + 2]];
            
            // Edge vectors
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            
            // Cross product for normal
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            
            // Normalize
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-10 {
                face_normals.push([n[0] / len, n[1] / len, n[2] / len]);
            } else {
                face_normals.push([0.0, 0.0, 1.0]);
            }
        }
        
        // Map each edge to the triangles that share it
        let mut edge_tris: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for tri in 0..num_tris {
            let i0 = self.indices[tri * 3] as usize;
            let i1 = self.indices[tri * 3 + 1] as usize;
            let i2 = self.indices[tri * 3 + 2] as usize;

            // Normalize edge representation (smaller index first)
            let e0 = if i0 < i1 { (i0, i1) } else { (i1, i0) };
            let e1 = if i1 < i2 { (i1, i2) } else { (i2, i1) };
            let e2 = if i2 < i0 { (i2, i0) } else { (i0, i2) };

            edge_tris.entry(e0).or_insert_with(Vec::new).push(tri);
            edge_tris.entry(e1).or_insert_with(Vec::new).push(tri);
            edge_tris.entry(e2).or_insert_with(Vec::new).push(tri);
        }

        // View direction for silhouette detection (normal to the projection plane)
        let view_dir: [f32; 3] = match plane {
            0 => [0.0, 1.0, 0.0],  // XZ plane: looking along Y
            1 => [0.0, 0.0, 1.0],  // XY plane: looking along Z (top/bottom)
            2 => [1.0, 0.0, 0.0],  // YZ plane: looking along X
            _ => [0.0, 0.0, 1.0],
        };

        // Keep edges that are: boundary (1 tri) OR crease OR silhouette
        let edges: Vec<(usize, usize)> = edge_tris
            .into_iter()
            .filter(|(_, tris)| {
                if tris.len() == 1 {
                    return true; // Boundary edge
                }
                if tris.len() == 2 {
                    let n1 = face_normals[tris[0]];
                    let n2 = face_normals[tris[1]];
                    
                    // Crease detection: normals differ significantly
                    let dot = n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2];
                    if dot < CREASE_THRESHOLD {
                        return true;
                    }
                    
                    // Silhouette detection: one face front-facing, other back-facing
                    let d1 = n1[0] * view_dir[0] + n1[1] * view_dir[1] + n1[2] * view_dir[2];
                    let d2 = n2[0] * view_dir[0] + n2[1] * view_dir[1] + n2[2] * view_dir[2];
                    if d1 * d2 < 0.0 {
                        return true; // One front-facing, one back-facing
                    }
                }
                false // Non-manifold edge (>2 tris)
            })
            .map(|(edge, _)| edge)
            .collect();

        // Project edges onto the specified plane
        let mut projected = Vec::new();
        for (i0, i1) in edges {
            if i0 >= self.positions.len() / 3 || i1 >= self.positions.len() / 3 {
                continue;
            }

            let p0 = [
                self.positions[i0 * 3],
                self.positions[i0 * 3 + 1],
                self.positions[i0 * 3 + 2],
            ];
            let p1 = [
                self.positions[i1 * 3],
                self.positions[i1 * 3 + 1],
                self.positions[i1 * 3 + 2],
            ];

            // Project to 2D based on plane
            let (x0, y0, x1, y1) = match plane {
                0 => (p0[0], p0[2], p1[0], p1[2]), // XZ plane (X, Z)
                1 => (p0[0], p0[1], p1[0], p1[1]), // XY plane (X, Y)
                2 => (p0[1], p0[2], p1[1], p1[2]), // YZ plane (Y, Z)
                _ => continue,
            };

            projected.push([x0, y0, x1, y1]);
        }

        // Build binary message
        let plane_names = ["BP_XZ\0\0\0", "BP_XY\0\0\0", "BP_YZ\0\0\0"];
        let header = plane_names[plane as usize];

        let mut data = Vec::new();
        data.extend_from_slice(header.as_bytes());
        data.extend_from_slice(&(projected.len() as u32).to_le_bytes());

        for edge in projected {
            for &coord in &edge {
                data.extend_from_slice(&coord.to_le_bytes());
            }
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

    if obj_type == "group" || obj_type == "assembly" || obj_type == "component" {
        // Groups, assemblies, and components all union their children
        let children: mlua::Table = table.get("children")?;
        let mut child_meshes = Vec::new();

        for pair in children.pairs::<i64, mlua::Table>() {
            let (_, child) = pair?;
            let child_mesh = build_mesh_recursive_with_components(&child, circular_segments, components)?;
            child_meshes.push(child_mesh);
        }

        let mut combined = if child_meshes.is_empty() {
            return Err(anyhow!("Empty group/assembly/component"));
        } else {
            combine_meshes(child_meshes)
        };

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
