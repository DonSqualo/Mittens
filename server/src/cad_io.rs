use crate::geometry::MeshData;
use anyhow::{anyhow, bail, Context, Result};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Lua,
    Stl,
    Step,
}

pub fn detect_project_kind(path: &Path) -> Result<ProjectKind> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| anyhow!("project file has no extension: {}", path.display()))?;

    match ext.as_str() {
        "lua" => Ok(ProjectKind::Lua),
        "stl" => Ok(ProjectKind::Stl),
        "step" | "stp" => Ok(ProjectKind::Step),
        _ => bail!(
            "unsupported project extension '.{}' for file {} (supported: .lua .stl .step/.stp)",
            ext,
            path.display()
        ),
    }
}

pub fn load_mesh_from_path(path: &Path) -> Result<MeshData> {
    match detect_project_kind(path)? {
        ProjectKind::Stl => load_stl_mesh(path),
        ProjectKind::Step => load_step_mesh(path),
        ProjectKind::Lua => bail!("load_mesh_from_path only supports mesh files (.stl/.step/.stp)"),
    }
}

fn load_stl_mesh(path: &Path) -> Result<MeshData> {
    let file =
        File::open(path).with_context(|| format!("failed to open STL {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mesh = stl_io::read_stl(&mut reader)
        .with_context(|| format!("failed to read STL {}", path.display()))?;

    let mut out = MeshData::new_empty();
    out.positions.reserve(mesh.vertices.len() * 3);
    out.normals.resize(mesh.vertices.len() * 3, 0.0);
    out.indices.reserve(mesh.faces.len() * 3);

    for v in &mesh.vertices {
        out.positions.push(v.0[0]);
        out.positions.push(v.0[1]);
        out.positions.push(v.0[2]);
    }

    for face in &mesh.faces {
        let i0 = face.vertices[0] as u32;
        let i1 = face.vertices[1] as u32;
        let i2 = face.vertices[2] as u32;
        out.indices.push(i0);
        out.indices.push(i1);
        out.indices.push(i2);

        // Use facet normals for fast STL ingest, then normalize accumulated vertex normals.
        let nx = face.normal.0[0];
        let ny = face.normal.0[1];
        let nz = face.normal.0[2];
        for idx in [i0, i1, i2] {
            let base = idx as usize * 3;
            out.normals[base] += nx;
            out.normals[base + 1] += ny;
            out.normals[base + 2] += nz;
        }
    }

    normalize_normals(&mut out.normals);
    Ok(out)
}

#[cfg(feature = "occt-support")]
fn load_step_mesh(path: &Path) -> Result<MeshData> {
    use opencascade::primitives::Shape;

    let shape = Shape::read_step(path)
        .with_context(|| format!("failed to read STEP {}", path.display()))?;
    let mesh = shape.mesh();

    let mut out = MeshData::new_empty();
    out.positions.reserve(mesh.vertices.len() * 3);
    out.normals.reserve(mesh.vertices.len() * 3);
    out.indices.reserve(mesh.indices.len());

    for p in &mesh.vertices {
        out.positions.push(p.x as f32);
        out.positions.push(p.y as f32);
        out.positions.push(p.z as f32);
    }
    for n in &mesh.normals {
        out.normals.push(n.x as f32);
        out.normals.push(n.y as f32);
        out.normals.push(n.z as f32);
    }
    for &idx in &mesh.indices {
        out.indices.push(idx as u32);
    }

    if out.normals.len() != out.positions.len() {
        out.normals.resize(out.positions.len(), 0.0);
        recompute_vertex_normals(&out.positions, &out.indices, &mut out.normals);
    } else {
        normalize_normals(&mut out.normals);
    }

    Ok(out)
}

#[cfg(not(feature = "occt-support"))]
fn load_step_mesh(path: &Path) -> Result<MeshData> {
    let _ = path;
    bail!("STEP support disabled at compile time; rebuild server with feature 'occt-support'")
}

fn normalize_normals(normals: &mut [f32]) {
    for i in 0..(normals.len() / 3) {
        let nx = normals[i * 3];
        let ny = normals[i * 3 + 1];
        let nz = normals[i * 3 + 2];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-12 {
            normals[i * 3] = nx / len;
            normals[i * 3 + 1] = ny / len;
            normals[i * 3 + 2] = nz / len;
        }
    }
}

#[cfg(feature = "occt-support")]
fn recompute_vertex_normals(positions: &[f32], indices: &[u32], normals: &mut [f32]) {
    for tri in 0..(indices.len() / 3) {
        let i0 = indices[tri * 3] as usize;
        let i1 = indices[tri * 3 + 1] as usize;
        let i2 = indices[tri * 3 + 2] as usize;
        let p0 = [
            positions[i0 * 3],
            positions[i0 * 3 + 1],
            positions[i0 * 3 + 2],
        ];
        let p1 = [
            positions[i1 * 3],
            positions[i1 * 3 + 1],
            positions[i1 * 3 + 2],
        ];
        let p2 = [
            positions[i2 * 3],
            positions[i2 * 3 + 1],
            positions[i2 * 3 + 2],
        ];
        let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let nx = u[1] * v[2] - u[2] * v[1];
        let ny = u[2] * v[0] - u[0] * v[2];
        let nz = u[0] * v[1] - u[1] * v[0];
        for idx in [i0, i1, i2] {
            normals[idx * 3] += nx;
            normals[idx * 3 + 1] += ny;
            normals[idx * 3 + 2] += nz;
        }
    }
    normalize_normals(normals);
}
