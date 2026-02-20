//! Manifold-based CSG geometry backend
//! Uses manifold3d for guaranteed watertight manifold meshes

use anyhow::{anyhow, Result};
use crate::cad_io;
use crate::ir;
use crate::thread_primitives::{generate_external_thread, generate_internal_thread};
use manifold3d::types::{Matrix4x3, PositiveF64, PositiveI32, Vec3};
use manifold3d::{Manifold, MeshGL};
use mlua::{Lua, Value};
use serde_json::Value as JsonValue;
use std::alloc::{alloc, Layout};
use std::collections::HashMap;
use std::os::raw::c_void;
use std::path::Path;

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

fn mesh_data_to_manifold(mesh: &MeshData) -> Result<Manifold> {
    let num_verts = mesh.positions.len() / 3;
    let num_tris = mesh.indices.len() / 3;

    if num_verts == 0 || num_tris == 0 {
        return Err(anyhow!("mesh_file produced empty mesh"));
    }

    if mesh.positions.len() % 3 != 0 || mesh.indices.len() % 3 != 0 {
        return Err(anyhow!("mesh_file has invalid buffer lengths"));
    }

    let mut vert_props: Vec<f32> = Vec::with_capacity(num_verts * 6);
    for i in 0..num_verts {
        vert_props.push(mesh.positions[i * 3]);
        vert_props.push(mesh.positions[i * 3 + 1]);
        vert_props.push(mesh.positions[i * 3 + 2]);

        if mesh.normals.len() >= (i + 1) * 3 {
            vert_props.push(mesh.normals[i * 3]);
            vert_props.push(mesh.normals[i * 3 + 1]);
            vert_props.push(mesh.normals[i * 3 + 2]);
        } else {
            vert_props.extend_from_slice(&[0.0, 0.0, 1.0]);
        }
    }

    let manifold: Manifold = unsafe {
        let mesh_ptr = manifold_meshgl(
            manifold_alloc_meshgl(),
            vert_props.as_ptr(),
            num_verts,
            6,
            mesh.indices.as_ptr(),
            num_tris,
        );
        let manifold_ptr = manifold_of_meshgl(manifold_alloc_manifold(), mesh_ptr);
        std::mem::transmute(manifold_ptr)
    };

    Ok(manifold)
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
    tracing::debug!("build_manifold_primitive: obj_type='{}', circular_segments={}", obj_type, circular_segments);
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
        "text" => {
            let text_str: String = params.get("text")?;
            let font_size: f64 = params.get("size")?;
            
            // Generate text mesh
            generate_text_mesh(&text_str, font_size)
        }
        "external_thread" => {
            let major_diameter: f64 = params.get("major_diameter")?;
            let pitch: f64 = params.get("pitch").unwrap_or(3.0);
            let height: f64 = params.get("height")?;
            let segments_per_turn: usize = params.get::<_, i64>("segments_per_turn").unwrap_or(32) as usize;
            let clearance: f64 = params.get::<_, f64>("clearance").unwrap_or(0.0);

            let (vert_props, tri_verts) = generate_external_thread(
                major_diameter,
                pitch,
                height,
                segments_per_turn,
                clearance,
            );

            let actual_verts = vert_props.len() / 6;
            let num_tris = tri_verts.len() / 3;

            let result: Manifold = unsafe {
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

            Ok(result)
        }
        "internal_thread" => {
            let major_diameter: f64 = params.get("major_diameter")?;
            let pitch: f64 = params.get("pitch").unwrap_or(3.0);
            let height: f64 = params.get("height")?;
            let segments_per_turn: usize = params.get::<_, i64>("segments_per_turn").unwrap_or(32) as usize;
            let clearance: f64 = params.get::<_, f64>("clearance").unwrap_or(0.0);
            let thread_depth = 0.54125 * pitch;
            let wall_thickness: f64 = params
                .get::<_, f64>("wall_thickness")
                .unwrap_or(thread_depth * 5.0);

            let (vert_props, tri_verts) = generate_internal_thread(
                major_diameter,
                pitch,
                height,
                segments_per_turn,
                clearance,
                wall_thickness,
            );

            let actual_verts = vert_props.len() / 6;
            let num_tris = tri_verts.len() / 3;

            let result: Manifold = unsafe {
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

            Ok(result)
        }
        "mesh_file" => {
            let path_str: String = params.get("path")?;
            let mesh = cad_io::load_mesh_from_path(Path::new(&path_str))
                .map_err(|e| anyhow!("failed to load mesh_file '{}': {}", path_str, e))?;
            mesh_data_to_manifold(&mesh)
        }
        _ => Err(anyhow!("Unknown primitive type: {}", obj_type)),
    }
}

fn generate_text_mesh(text: &str, font_size: f64) -> Result<Manifold> {
    // Simple monospace font: each character is a 6x10 grid of unit squares
    // We'll create thin rectangles (extrusion in Y) for each character
    
    let char_width = font_size * 0.6;
    let char_height = font_size;
    let extrusion_thickness = 0.5; // Slight 3D extrusion in Y direction
    
    let mut vert_props: Vec<f32> = Vec::new();
    let mut tri_verts: Vec<u32> = Vec::new();
    let mut vertex_count = 0u32;
    
    // Simple character rasterization: for each character, draw a filled rectangle
    for (char_idx, ch) in text.chars().enumerate() {
        let x_offset = (char_idx as f64) * char_width;
        
        // Skip whitespace by rendering as empty space
        if ch == ' ' {
            continue;
        }
        
        // Create a rectangle for the character
        // Vertices for front face (Y=0)
        let positions = vec![
            (x_offset, 0.0, 0.0),                    // 0: front-bottom-left
            (x_offset + char_width, 0.0, 0.0),       // 1: front-bottom-right
            (x_offset + char_width, 0.0, char_height), // 2: front-top-right
            (x_offset, 0.0, char_height),            // 3: front-top-left
            // Vertices for back face (Y=extrusion_thickness)
            (x_offset, extrusion_thickness, 0.0),                    // 4: back-bottom-left
            (x_offset + char_width, extrusion_thickness, 0.0),       // 5: back-bottom-right
            (x_offset + char_width, extrusion_thickness, char_height), // 6: back-top-right
            (x_offset, extrusion_thickness, char_height),            // 7: back-top-left
        ];
        
        // Add vertices to the list (with dummy normals that will be computed later)
        for (px, py, pz) in positions {
            vert_props.extend_from_slice(&[px as f32, py as f32, pz as f32, 0.0, 0.0, 1.0]);
        }
        
        // Add triangles for the 6 faces of the box
        let base = vertex_count;
        
        // Front face (Y=0)
        tri_verts.extend_from_slice(&[base, base+1, base+2]);
        tri_verts.extend_from_slice(&[base, base+2, base+3]);
        
        // Back face (Y=thickness)
        tri_verts.extend_from_slice(&[base+4, base+6, base+5]);
        tri_verts.extend_from_slice(&[base+4, base+7, base+6]);
        
        // Top face (Z=height)
        tri_verts.extend_from_slice(&[base+3, base+2, base+6]);
        tri_verts.extend_from_slice(&[base+3, base+6, base+7]);
        
        // Bottom face (Z=0)
        tri_verts.extend_from_slice(&[base, base+5, base+1]);
        tri_verts.extend_from_slice(&[base, base+4, base+5]);
        
        // Left face (X=x_offset)
        tri_verts.extend_from_slice(&[base, base+3, base+7]);
        tri_verts.extend_from_slice(&[base, base+7, base+4]);
        
        // Right face (X=x_offset+char_width)
        tri_verts.extend_from_slice(&[base+1, base+5, base+6]);
        tri_verts.extend_from_slice(&[base+1, base+6, base+2]);
        
        vertex_count += 8;
    }
    
    if vert_props.is_empty() {
        // Return empty/minimal geometry if no text (all spaces)
        // Create a tiny dummy box to avoid invalid manifold
        let tiny = Manifold::new_cuboid(
            pos(0.01),
            pos(0.01),
            pos(0.01),
            false,
        );
        return Ok(tiny);
    }
    
    let num_verts = vertex_count as usize;
    let num_tris = tri_verts.len() / 3;
    
    // Create manifold from mesh
    let text_mesh: Manifold = unsafe {
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
    
    Ok(text_mesh)
}

fn apply_manifold_ops(manifold: Manifold, table: &mlua::Table) -> Result<Manifold> {
    let mut result = manifold;

    if let Ok(ops) = table.get::<_, mlua::Table>("ops") {
        for op_item in ops.sequence_values::<mlua::Table>() {
            if let Ok(op_table) = op_item {
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
    tracing::debug!("build_mesh_recursive_with_components: obj_type='{}'", obj_type);

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
    for op_item in ops.clone().sequence_values::<mlua::Table>() {
        if let Ok(op_table) = op_item {
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
    let scene = ir::scene_from_lua_value(value)?;
    let mut combined = generate_mesh_from_ir_scene(_lua, &scene, circular_segments)?;
    if remove_degenerates {
        let removed = remove_degenerate_triangles(&mut combined);
        if removed > 0 {
            tracing::debug!("Removed {} degenerate triangles from scene", removed);
        }
    }
    Ok(combined)
}

fn manifold_transform_from_ir(transform: &ir::TransformIr) -> Matrix4x3 {
    let m = transform.matrix;
    Matrix4x3::new([
        Vec3::new(m[0][0], m[1][0], m[2][0]),
        Vec3::new(m[0][1], m[1][1], m[2][1]),
        Vec3::new(m[0][2], m[1][2], m[2][2]),
        Vec3::new(m[0][3], m[1][3], m[2][3]),
    ])
}

fn apply_manifold_transform_ir(manifold: Manifold, transform: &ir::TransformIr) -> Manifold {
    manifold.transform(manifold_transform_from_ir(transform))
}

fn apply_mesh_transform_ir(mesh: &mut MeshData, transform: &ir::TransformIr) {
    let m = transform.matrix;
    let lin = [
        [m[0][0], m[0][1], m[0][2]],
        [m[1][0], m[1][1], m[1][2]],
        [m[2][0], m[2][1], m[2][2]],
    ];

    let det = lin[0][0] * (lin[1][1] * lin[2][2] - lin[1][2] * lin[2][1])
        - lin[0][1] * (lin[1][0] * lin[2][2] - lin[1][2] * lin[2][0])
        + lin[0][2] * (lin[1][0] * lin[2][1] - lin[1][1] * lin[2][0]);

    let normal_matrix = if det.abs() > 1e-12 {
        let inv_det = 1.0 / det;
        let inv = [
            [
                (lin[1][1] * lin[2][2] - lin[1][2] * lin[2][1]) * inv_det,
                (lin[0][2] * lin[2][1] - lin[0][1] * lin[2][2]) * inv_det,
                (lin[0][1] * lin[1][2] - lin[0][2] * lin[1][1]) * inv_det,
            ],
            [
                (lin[1][2] * lin[2][0] - lin[1][0] * lin[2][2]) * inv_det,
                (lin[0][0] * lin[2][2] - lin[0][2] * lin[2][0]) * inv_det,
                (lin[0][2] * lin[1][0] - lin[0][0] * lin[1][2]) * inv_det,
            ],
            [
                (lin[1][0] * lin[2][1] - lin[1][1] * lin[2][0]) * inv_det,
                (lin[0][1] * lin[2][0] - lin[0][0] * lin[2][1]) * inv_det,
                (lin[0][0] * lin[1][1] - lin[0][1] * lin[1][0]) * inv_det,
            ],
        ];
        Some([
            [inv[0][0], inv[1][0], inv[2][0]],
            [inv[0][1], inv[1][1], inv[2][1]],
            [inv[0][2], inv[1][2], inv[2][2]],
        ])
    } else {
        None
    };

    for i in 0..mesh.positions.len() / 3 {
        let px = mesh.positions[i * 3] as f64;
        let py = mesh.positions[i * 3 + 1] as f64;
        let pz = mesh.positions[i * 3 + 2] as f64;
        mesh.positions[i * 3] = (m[0][0] * px + m[0][1] * py + m[0][2] * pz + m[0][3]) as f32;
        mesh.positions[i * 3 + 1] = (m[1][0] * px + m[1][1] * py + m[1][2] * pz + m[1][3]) as f32;
        mesh.positions[i * 3 + 2] = (m[2][0] * px + m[2][1] * py + m[2][2] * pz + m[2][3]) as f32;
    }

    for i in 0..mesh.normals.len() / 3 {
        let nx = mesh.normals[i * 3] as f64;
        let ny = mesh.normals[i * 3 + 1] as f64;
        let nz = mesh.normals[i * 3 + 2] as f64;
        let (tx, ty, tz) = if let Some(nm) = normal_matrix {
            (
                nm[0][0] * nx + nm[0][1] * ny + nm[0][2] * nz,
                nm[1][0] * nx + nm[1][1] * ny + nm[1][2] * nz,
                nm[2][0] * nx + nm[2][1] * ny + nm[2][2] * nz,
            )
        } else {
            (
                lin[0][0] * nx + lin[0][1] * ny + lin[0][2] * nz,
                lin[1][0] * nx + lin[1][1] * ny + lin[1][2] * nz,
                lin[2][0] * nx + lin[2][1] * ny + lin[2][2] * nz,
            )
        };
        let len = (tx * tx + ty * ty + tz * tz).sqrt();
        if len > 1e-12 {
            mesh.normals[i * 3] = (tx / len) as f32;
            mesh.normals[i * 3 + 1] = (ty / len) as f32;
            mesh.normals[i * 3 + 2] = (tz / len) as f32;
        }
    }
}

fn parse_rgb_array(value: &JsonValue) -> Option<(f32, f32, f32)> {
    let arr = value.as_array()?;
    if arr.len() < 3 {
        return None;
    }
    Some((
        arr[0].as_f64()? as f32,
        arr[1].as_f64()? as f32,
        arr[2].as_f64()? as f32,
    ))
}

fn get_material_color_ir(obj: &ir::ObjectIr) -> Option<(f32, f32, f32)> {
    if let Some(color) = &obj.color {
        if let Some(rgb) = parse_rgb_array(color) {
            return Some(rgb);
        }
    }
    if let Some(material) = &obj.material {
        if let Some(color) = material.as_object().and_then(|m| m.get("color")) {
            if let Some(rgb) = parse_rgb_array(color) {
                return Some(rgb);
            }
        }
    }
    None
}

fn json_to_lua_value<'lua>(lua: &'lua Lua, value: &JsonValue) -> Result<Value<'lua>> {
    Ok(match value {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Boolean(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else {
                Value::Number(n.as_f64().unwrap_or(0.0))
            }
        }
        JsonValue::String(s) => Value::String(lua.create_string(s)?),
        JsonValue::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.set((i + 1) as i64, json_to_lua_value(lua, v)?)?;
            }
            Value::Table(t)
        }
        JsonValue::Object(map) => {
            let t = lua.create_table()?;
            for (k, v) in map {
                t.set(k.as_str(), json_to_lua_value(lua, v)?)?;
            }
            Value::Table(t)
        }
    })
}

fn build_manifold_primitive_from_ir(lua: &Lua, obj: &ir::ObjectIr, circular_segments: u32) -> Result<Manifold> {
    let params = obj
        .params
        .as_ref()
        .ok_or_else(|| anyhow!("Primitive '{}' missing params", obj.obj_type))?;
    let params_table = match json_to_lua_value(lua, params)? {
        Value::Table(t) => t,
        _ => return Err(anyhow!("Primitive '{}' params must be table-like", obj.obj_type)),
    };
    build_manifold_primitive(&obj.obj_type, &params_table, circular_segments)
}

fn collect_ir_components(obj: &ir::ObjectIr, out: &mut HashMap<String, ir::ObjectIr>) {
    if obj.obj_type == "component" {
        if let Some(name) = &obj.name {
            out.insert(name.clone(), obj.clone());
        }
    }
    for child in &obj.children {
        collect_ir_components(child, out);
    }
}

fn component_map_from_ir_scene(scene: &ir::SceneIr) -> HashMap<String, ir::ObjectIr> {
    let mut components = HashMap::new();
    for obj in &scene.objects {
        collect_ir_components(obj, &mut components);
    }
    components
}

fn build_manifold_object_from_ir(
    lua: &Lua,
    obj: &ir::ObjectIr,
    circular_segments: u32,
    components: &HashMap<String, ir::ObjectIr>,
) -> Result<Manifold> {
    let manifold = if obj.obj_type == "csg" {
        let operation = obj
            .operation
            .as_deref()
            .ok_or_else(|| anyhow!("CSG object missing operation"))?;
        let first = obj
            .children
            .first()
            .ok_or_else(|| anyhow!("CSG object missing children"))?;
        let mut result = build_manifold_object_from_ir(lua, first, circular_segments, components)?;
        for child in obj.children.iter().skip(1) {
            let child_m = build_manifold_object_from_ir(lua, child, circular_segments, components)?;
            result = match operation {
                "union" => result.union(&child_m),
                "difference" => result.difference(&child_m),
                "intersect" => result.intersection(&child_m),
                _ => return Err(anyhow!("Unknown CSG operation: {}", operation)),
            };
        }
        result
    } else if obj.obj_type == "group" || obj.obj_type == "assembly" || obj.obj_type == "component" {
        let mut result: Option<Manifold> = None;
        for child in &obj.children {
            let child_m = build_manifold_object_from_ir(lua, child, circular_segments, components)?;
            result = Some(match result {
                Some(r) => r.union(&child_m),
                None => child_m,
            });
        }
        result.ok_or_else(|| anyhow!("Empty {}", obj.obj_type))?
    } else if obj.obj_type == "instance" {
        let component_name = obj
            .component
            .as_deref()
            .ok_or_else(|| anyhow!("instance missing component reference"))?;
        let component = components
            .get(component_name)
            .ok_or_else(|| anyhow!("Component '{}' not found for instance", component_name))?;
        build_manifold_object_from_ir(lua, component, circular_segments, components)?
    } else if obj.obj_type == "mesh_file" {
        let params = obj
            .params
            .as_ref()
            .ok_or_else(|| anyhow!("mesh_file missing params"))?;
        let path_str = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("mesh_file params.path must be a string"))?;

        let mesh = cad_io::load_mesh_from_path(Path::new(path_str))
            .map_err(|e| anyhow!("failed to load mesh_file '{}': {}", path_str, e))?;
        mesh_data_to_manifold(&mesh)?
    } else {
        build_manifold_primitive_from_ir(lua, obj, circular_segments)?
    };

    Ok(apply_manifold_transform_ir(manifold, &obj.transform))
}

fn build_mesh_recursive_from_ir(
    lua: &Lua,
    obj: &ir::ObjectIr,
    circular_segments: u32,
    components: &HashMap<String, ir::ObjectIr>,
) -> Result<MeshData> {
    if obj.obj_type == "group" || obj.obj_type == "assembly" || obj.obj_type == "component" {
        let mut child_meshes = Vec::new();
        for child in &obj.children {
            child_meshes.push(build_mesh_recursive_from_ir(lua, child, circular_segments, components)?);
        }
        if child_meshes.is_empty() {
            return Err(anyhow!("Empty {}", obj.obj_type));
        }
        let mut combined = combine_meshes(child_meshes);
        if let Some((r, g, b)) = get_material_color_ir(obj) {
            apply_color_to_mesh(&mut combined, r, g, b);
        }
        apply_mesh_transform_ir(&mut combined, &obj.transform);
        Ok(combined)
    } else if obj.obj_type == "instance" {
        let component_name = obj
            .component
            .as_deref()
            .ok_or_else(|| anyhow!("instance missing component reference"))?;
        let component = components
            .get(component_name)
            .ok_or_else(|| anyhow!("Component '{}' not found for instance", component_name))?;
        let mut mesh = build_mesh_recursive_from_ir(lua, component, circular_segments, components)?;
        apply_mesh_transform_ir(&mut mesh, &obj.transform);
        Ok(mesh)
    } else if obj.obj_type == "csg" {
        let manifold = build_manifold_object_from_ir(lua, obj, circular_segments, components)?;
        let mut mesh = manifold_to_mesh_data(&manifold);
        if let Some((r, g, b)) = get_material_color_ir(obj) {
            apply_color_to_mesh(&mut mesh, r, g, b);
        } else if let Some(first_child) = obj.children.first() {
            if let Some((r, g, b)) = get_material_color_ir(first_child) {
                apply_color_to_mesh(&mut mesh, r, g, b);
            }
        }
        Ok(mesh)
    } else if obj.obj_type == "mesh_file" {
        let params = obj
            .params
            .as_ref()
            .ok_or_else(|| anyhow!("mesh_file missing params"))?;
        let path_str = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("mesh_file params.path must be a string"))?;

        let mut mesh = cad_io::load_mesh_from_path(Path::new(path_str))
            .map_err(|e| anyhow!("failed to load mesh_file '{}': {}", path_str, e))?;

        if let Some((r, g, b)) = get_material_color_ir(obj) {
            apply_color_to_mesh(&mut mesh, r, g, b);
        }
        apply_mesh_transform_ir(&mut mesh, &obj.transform);
        Ok(mesh)
    } else {
        let manifold = build_manifold_primitive_from_ir(lua, obj, circular_segments)?;
        let manifold = apply_manifold_transform_ir(manifold, &obj.transform);
        let mut mesh = manifold_to_mesh_data(&manifold);
        if let Some((r, g, b)) = get_material_color_ir(obj) {
            apply_color_to_mesh(&mut mesh, r, g, b);
        }
        Ok(mesh)
    }
}

/// Generate mesh from canonical IR scene using existing Manifold primitive/boolean semantics.
pub fn generate_mesh_from_ir_scene(
    lua: &Lua,
    scene: &ir::SceneIr,
    circular_segments: u32,
) -> Result<MeshData> {
    let components = component_map_from_ir_scene(scene);
    let mut meshes = Vec::new();

    for obj in &scene.objects {
        meshes.push(build_mesh_recursive_from_ir(lua, obj, circular_segments, &components)?);
    }

    if meshes.is_empty() {
        return Err(anyhow!("No objects in scene"));
    }
    let mut mesh = combine_meshes(meshes);
    remove_degenerate_triangles(&mut mesh);
    Ok(mesh)
}

/// Generate mesh from one canonical IR object, optionally resolving instances against scene components.
pub fn generate_mesh_from_ir_object(
    lua: &Lua,
    obj: &ir::ObjectIr,
    scene: Option<&ir::SceneIr>,
    circular_segments: u32,
) -> Result<MeshData> {
    let components = scene.map(component_map_from_ir_scene).unwrap_or_default();
    let mut mesh = build_mesh_recursive_from_ir(lua, obj, circular_segments, &components)?;
    remove_degenerate_triangles(&mut mesh);
    Ok(mesh)
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
