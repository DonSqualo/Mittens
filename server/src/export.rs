//! Export - STL and 3MF file generation for 3D printing
//! Units: millimeters, manifold meshes

use crate::ir;
use crate::geometry::MeshData;
use chrono::Local;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use tracing::info;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Write binary STL file from mesh data
pub fn write_stl(mesh: &MeshData, path: &Path) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    // 80-byte header (padded with zeros)
    let mut header = [0u8; 80];
    let text = b"Mittens STL - units: mm";
    header[..text.len()].copy_from_slice(text);
    writer.write_all(&header)?;

    // Triangle count
    let num_triangles = (mesh.indices.len() / 3) as u32;
    writer.write_all(&num_triangles.to_le_bytes())?;

    // Write each triangle
    for tri in 0..(mesh.indices.len() / 3) {
        let i0 = mesh.indices[tri * 3] as usize;
        let i1 = mesh.indices[tri * 3 + 1] as usize;
        let i2 = mesh.indices[tri * 3 + 2] as usize;

        // Get vertices
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

        // Compute face normal from vertices (right-hand rule)
        let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let normal = cross(edge1, edge2);
        let normal = normalize(normal);

        // Write normal
        for n in normal {
            writer.write_all(&n.to_le_bytes())?;
        }

        // Write vertices
        for v in [v0, v1, v2] {
            for coord in v {
                writer.write_all(&coord.to_le_bytes())?;
            }
        }

        // Attribute byte count (unused)
        writer.write_all(&0u16.to_le_bytes())?;
    }

    writer.flush()?;
    info!("Exported STL: {} triangles to {:?}", num_triangles, path);
    Ok(())
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-10 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

// ===========================

/// Write 3MF file from mesh data with optional per-vertex colors
/// 3MF is a ZIP archive containing XML model data - widely supported by slicers
pub fn write_3mf(
    mesh: &MeshData,
    path: &Path,
    units: &str,
    include_colors: bool,
) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // [Content_Types].xml - declares MIME types for 3MF
    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(CONTENT_TYPES_XML.as_bytes())?;

    // _rels/.rels - root relationships
    zip.start_file("_rels/.rels", options)?;
    zip.write_all(RELS_XML.as_bytes())?;

    // 3D/3dmodel.model - the actual mesh data
    zip.start_file("3D/3dmodel.model", options)?;
    let model_xml = build_model_xml(mesh, units, include_colors);
    zip.write_all(model_xml.as_bytes())?;

    zip.finish()?;
    let num_triangles = mesh.indices.len() / 3;
    info!(
        "Exported 3MF: {} triangles, colors={} to {:?}",
        num_triangles, include_colors, path
    );
    Ok(())
}

const CONTENT_TYPES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>"#;

const RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>"#;

fn build_model_xml(mesh: &MeshData, units: &str, include_colors: bool) -> String {
    let mut xml = String::with_capacity(mesh.positions.len() * 50);

    // XML header and model element
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(r#"<model unit=""#);
    xml.push_str(units);
    xml.push_str(r#"" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02""#);

    // Add materials namespace if colors included
    if include_colors && !mesh.colors.is_empty() {
        xml.push_str(r#" xmlns:m="http://schemas.microsoft.com/3dmanufacturing/material/2015/02""#);
    }
    xml.push_str(">\n");

    // Resources section
    xml.push_str("  <resources>\n");

    // Add color materials if present
    let has_colors = include_colors && mesh.colors.len() >= mesh.positions.len();
    if has_colors {
        xml.push_str("    <m:colorgroup id=\"1\">\n");
        let num_vertices = mesh.positions.len() / 3;
        for i in 0..num_vertices {
            let r = (mesh.colors[i * 3] * 255.0).round() as u8;
            let g = (mesh.colors[i * 3 + 1] * 255.0).round() as u8;
            let b = (mesh.colors[i * 3 + 2] * 255.0).round() as u8;
            xml.push_str(&format!(
                "      <m:color color=\"#{:02X}{:02X}{:02X}\"/>\n",
                r, g, b
            ));
        }
        xml.push_str("    </m:colorgroup>\n");
    }

    // Object with mesh
    xml.push_str("    <object id=\"1\" type=\"model\">\n");
    xml.push_str("      <mesh>\n");

    // Vertices
    xml.push_str("        <vertices>\n");
    let num_vertices = mesh.positions.len() / 3;
    for i in 0..num_vertices {
        let x = mesh.positions[i * 3];
        let y = mesh.positions[i * 3 + 1];
        let z = mesh.positions[i * 3 + 2];
        xml.push_str(&format!(
            "          <vertex x=\"{:.6}\" y=\"{:.6}\" z=\"{:.6}\"/>\n",
            x, y, z
        ));
    }
    xml.push_str("        </vertices>\n");

    // Triangles
    xml.push_str("        <triangles>\n");
    let num_triangles = mesh.indices.len() / 3;
    for i in 0..num_triangles {
        let v1 = mesh.indices[i * 3];
        let v2 = mesh.indices[i * 3 + 1];
        let v3 = mesh.indices[i * 3 + 2];

        if has_colors {
            // Reference vertex colors via p1/p2/p3 attributes
            xml.push_str(&format!(
                "          <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" pid=\"1\" p1=\"{}\" p2=\"{}\" p3=\"{}\"/>\n",
                v1, v2, v3, v1, v2, v3
            ));
        } else {
            xml.push_str(&format!(
                "          <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>\n",
                v1, v2, v3
            ));
        }
    }
    xml.push_str("        </triangles>\n");

    xml.push_str("      </mesh>\n");
    xml.push_str("    </object>\n");
    xml.push_str("  </resources>\n");

    // Build section - instantiate the object
    xml.push_str("  <build>\n");
    xml.push_str("    <item objectid=\"1\"/>\n");
    xml.push_str("  </build>\n");

    xml.push_str("</model>");
    xml
}

/// Process exports from the result table using Manifold backend
/// Supports STL (binary) and 3MF (ZIP with XML, optional colors)
pub fn process_exports_from_table(
    lua: &mlua::Lua,
    table: &mlua::Table,
    scene_ir: &ir::SceneIr,
    base_dir: &Path,
) -> Vec<String> {
    use crate::geometry;

    let exports = match table.get::<_, mlua::Table>("exports") {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut stl_outputs: Vec<String> = Vec::new();

    for pair in exports.pairs::<i32, mlua::Table>() {
        if let Ok((_, exp)) = pair {
            let format: String = exp.get("format").unwrap_or_default();
            let filename: String = exp.get("filename").unwrap_or_default();

            if filename.is_empty() {
                continue;
            }

            let object: mlua::Table = match exp.get("object") {
                Ok(o) => o,
                Err(_) => continue,
            };
            let ir_object = match ir::object_from_lua_table(&object) {
                Ok(obj) => obj,
                Err(e) => {
                    tracing::error!("IR conversion failed for export {}: {}", filename, e);
                    continue;
                }
            };

            let circular_segments: u32 = exp.get("circular_segments").unwrap_or(128);
            let path = base_dir.join(&filename);

            match format.as_str() {
                "stl" => {
                    let output_path = versioned_stl_path(&path);
                    match geometry::generate_mesh_from_ir_object(
                        lua,
                        &ir_object,
                        Some(scene_ir),
                        circular_segments,
                    ) {
                        Ok(mesh) => {
                            if let Err(e) = write_stl(&mesh, &output_path) {
                                tracing::error!("STL export failed for {}: {}", filename, e);
                            } else {
                                if let Ok(rel) = output_path.strip_prefix(base_dir) {
                                    stl_outputs.push(rel.to_string_lossy().to_string());
                                } else {
                                    stl_outputs.push(
                                        output_path
                                            .file_name()
                                            .map(|n| n.to_string_lossy().to_string())
                                            .unwrap_or_else(|| output_path.to_string_lossy().to_string()),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Mesh generation failed for {}: {}", filename, e);
                        }
                    }
                }
                "3mf" => {
                    let units: String = exp.get("units").unwrap_or_else(|_| "millimeter".to_string());
                    let include_colors: bool = exp.get("include_colors").unwrap_or(true);
                    match geometry::generate_mesh_from_ir_object(
                        lua,
                        &ir_object,
                        Some(scene_ir),
                        circular_segments,
                    ) {
                        Ok(mesh) => {
                            if let Err(e) = write_3mf(&mesh, &path, &units, include_colors) {
                                tracing::error!("3MF export failed for {}: {}", filename, e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Mesh generation failed for {}: {}", filename, e);
                        }
                    }
                }
                _ => {
                    tracing::warn!("Unsupported export format: {}", format);
                }
            }
        }
    }
    stl_outputs
}

fn versioned_stl_path(path: &Path) -> PathBuf {
    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "export".to_string());
    let name = format!("{}_{}.stl", stem, ts);
    path.with_file_name(name)
}

// ===========================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;
    use zip::ZipArchive;

    fn make_test_cube() -> MeshData {
        // Simple cube: 8 vertices, 12 triangles (2 per face)
        // Corner normals computed as average of adjacent face normals
        // For cube corners, normalized (-1,-1,-1) to (+1,+1,+1) diagonal
        let inv_sqrt3: f32 = 1.0 / 3.0_f32.sqrt();
        MeshData {
            positions: vec![
                // Front face (z=0)
                0.0, 0.0, 0.0, // 0: corner (-x,-y,-z)
                10.0, 0.0, 0.0, // 1: corner (+x,-y,-z)
                10.0, 10.0, 0.0, // 2: corner (+x,+y,-z)
                0.0, 10.0, 0.0, // 3: corner (-x,+y,-z)
                // Back face (z=10)
                0.0, 0.0, 10.0, // 4: corner (-x,-y,+z)
                10.0, 0.0, 10.0, // 5: corner (+x,-y,+z)
                10.0, 10.0, 10.0, // 6: corner (+x,+y,+z)
                0.0, 10.0, 10.0, // 7: corner (-x,+y,+z)
            ],
            normals: vec![
                // Corner normals (averaged from 3 adjacent faces)
                -inv_sqrt3, -inv_sqrt3, -inv_sqrt3, // 0
                inv_sqrt3, -inv_sqrt3, -inv_sqrt3,  // 1
                inv_sqrt3, inv_sqrt3, -inv_sqrt3,   // 2
                -inv_sqrt3, inv_sqrt3, -inv_sqrt3,  // 3
                -inv_sqrt3, -inv_sqrt3, inv_sqrt3,  // 4
                inv_sqrt3, -inv_sqrt3, inv_sqrt3,   // 5
                inv_sqrt3, inv_sqrt3, inv_sqrt3,    // 6
                -inv_sqrt3, inv_sqrt3, inv_sqrt3,   // 7
            ],
            colors: vec![
                1.0, 0.0, 0.0, // Red
                0.0, 1.0, 0.0, // Green
                0.0, 0.0, 1.0, // Blue
                1.0, 1.0, 0.0, // Yellow
                1.0, 0.0, 1.0, // Magenta
                0.0, 1.0, 1.0, // Cyan
                1.0, 1.0, 1.0, // White
                0.5, 0.5, 0.5, // Gray
            ],
            indices: vec![
                // Front (z=0, normal -z)
                0, 1, 2, 0, 2, 3,
                // Back (z=10, normal +z)
                4, 6, 5, 4, 7, 6,
                // Top (y=10, normal +y)
                3, 2, 6, 3, 6, 7,
                // Bottom (y=0, normal -y)
                0, 5, 1, 0, 4, 5,
                // Right (x=10, normal +x)
                1, 5, 6, 1, 6, 2,
                // Left (x=0, normal -x)
                0, 3, 7, 0, 7, 4,
            ],
        }
    }

    #[test]
    fn test_stl_export_creates_valid_file() {
        let mesh = make_test_cube();
        let path = std::env::temp_dir().join("test_cube.stl");

        write_stl(&mesh, &path).expect("STL export failed");

        // Verify file exists and has correct structure
        let data = fs::read(&path).expect("Failed to read STL");

        // Binary STL: 80 byte header + 4 byte triangle count + 50 bytes per triangle
        let num_triangles = mesh.indices.len() / 3;
        let expected_size = 80 + 4 + (num_triangles * 50);
        assert_eq!(data.len(), expected_size, "STL file size mismatch");

        // Check header
        assert!(
            data[..25].starts_with(b"Mittens STL"),
            "STL header mismatch"
        );

        // Check triangle count
        let tri_count = u32::from_le_bytes([data[80], data[81], data[82], data[83]]);
        assert_eq!(tri_count as usize, num_triangles, "Triangle count mismatch");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_3mf_export_creates_valid_archive() {
        let mesh = make_test_cube();
        let path = std::env::temp_dir().join("test_cube.3mf");

        write_3mf(&mesh, &path, "millimeter", false).expect("3MF export failed");

        // Verify it's a valid ZIP
        let file = fs::File::open(&path).expect("Failed to open 3MF");
        let mut archive = ZipArchive::new(file).expect("Invalid ZIP archive");

        // Check required files exist
        let required_files = ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"];
        for name in &required_files {
            archive
                .by_name(name)
                .unwrap_or_else(|_| panic!("Missing required file: {}", name));
        }

        // Verify model XML structure
        let mut model_file = archive.by_name("3D/3dmodel.model").unwrap();
        let mut model_xml = String::new();
        model_file.read_to_string(&mut model_xml).unwrap();

        assert!(model_xml.contains("<model"), "Missing model element");
        assert!(model_xml.contains("<vertices>"), "Missing vertices");
        assert!(model_xml.contains("<triangles>"), "Missing triangles");
        assert!(
            model_xml.contains("unit=\"millimeter\""),
            "Missing units attribute"
        );

        // Check vertex count
        let vertex_count = model_xml.matches("<vertex").count();
        assert_eq!(vertex_count, 8, "Vertex count mismatch");

        // Check triangle count (match exact tag, not <triangles>)
        let triangle_count = model_xml.matches("<triangle ").count();
        assert_eq!(triangle_count, 12, "Triangle count mismatch");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_3mf_export_with_colors() {
        let mesh = make_test_cube();
        let path = std::env::temp_dir().join("test_cube_colors.3mf");

        write_3mf(&mesh, &path, "millimeter", true).expect("3MF export failed");

        let file = fs::File::open(&path).expect("Failed to open 3MF");
        let mut archive = ZipArchive::new(file).expect("Invalid ZIP archive");

        let mut model_file = archive.by_name("3D/3dmodel.model").unwrap();
        let mut model_xml = String::new();
        model_file.read_to_string(&mut model_xml).unwrap();

        // Verify color group exists
        assert!(
            model_xml.contains("<m:colorgroup"),
            "Missing colorgroup for colored export"
        );
        assert!(model_xml.contains("<m:color"), "Missing color definitions");

        // Verify triangles reference colors (pid attribute)
        assert!(
            model_xml.contains("pid=\"1\""),
            "Triangles missing color reference"
        );

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_cross_product() {
        let a = [1.0, 0.0, 0.0];
        let b = [0.0, 1.0, 0.0];
        let result = cross(a, b);
        assert!((result[0] - 0.0).abs() < 1e-6);
        assert!((result[1] - 0.0).abs() < 1e-6);
        assert!((result[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize() {
        let v = [3.0, 4.0, 0.0];
        let result = normalize(v);
        assert!((result[0] - 0.6).abs() < 1e-6);
        assert!((result[1] - 0.8).abs() < 1e-6);
        assert!((result[2] - 0.0).abs() < 1e-6);

        // Test zero vector fallback
        let zero = [0.0, 0.0, 0.0];
        let result = normalize(zero);
        assert_eq!(result, [0.0, 0.0, 1.0]);
    }
}
