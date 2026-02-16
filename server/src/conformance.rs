use anyhow::{anyhow, Result};
use serde::Serialize;
use std::cmp::Ordering;
use std::fs;
use std::path::Path;

use crate::geometry::MeshData;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct OracleConfig {
    pub surface_samples: usize,
    pub volume_samples: usize,
    pub boundary_band_mm: f32,
    pub max_surface_p95_mm: f32,
    pub max_inside_disagreement_rate: f64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            surface_samples: 2000,
            volume_samples: 6000,
            boundary_band_mm: 0.05,
            max_surface_p95_mm: 0.15,
            max_inside_disagreement_rate: 0.01,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SurfaceDistanceMetrics {
    pub max_mm: f32,
    pub p95_mm: f32,
    pub mean_mm: f32,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct InsideOutsideMetrics {
    pub total_samples: usize,
    pub disagreements: usize,
    pub disagreement_rate: f64,
    pub disagreements_outside_boundary_band: usize,
    pub disagreement_rate_outside_boundary_band: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct VolumeMetrics {
    pub bbox_volume_mm3: f64,
    pub volume_a_mm3: f64,
    pub volume_b_mm3: f64,
    pub abs_delta_mm3: f64,
    pub rel_delta: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct BoundsMetrics {
    pub min_delta_mm: [f32; 3],
    pub max_delta_mm: [f32; 3],
    pub max_abs_delta_mm: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct OracleReport {
    pub config: OracleConfig,
    pub surface_a_to_b: SurfaceDistanceMetrics,
    pub surface_b_to_a: SurfaceDistanceMetrics,
    pub symmetric_max_mm: f32,
    pub symmetric_p95_mm: f32,
    pub inside_outside: InsideOutsideMetrics,
    pub volume: VolumeMetrics,
    pub bounds: BoundsMetrics,
    pub pass: bool,
    pub failed_checks: Vec<String>,
}

#[derive(Clone, Copy)]
struct Triangle {
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    area: f32,
}

#[derive(Clone, Copy)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Clone)]
struct PreparedMesh {
    triangles: Vec<Triangle>,
    bounds: Bounds,
}

pub fn write_binary_stl(mesh: &MeshData, path: &Path) -> Result<()> {
    let mut data = Vec::new();
    let mut header = [0u8; 80];
    let text = b"Mittens Conformance STL";
    header[..text.len()].copy_from_slice(text);
    data.extend_from_slice(&header);

    let tri_count = (mesh.indices.len() / 3) as u32;
    data.extend_from_slice(&tri_count.to_le_bytes());

    for tri in 0..(mesh.indices.len() / 3) {
        let i0 = mesh.indices[tri * 3] as usize;
        let i1 = mesh.indices[tri * 3 + 1] as usize;
        let i2 = mesh.indices[tri * 3 + 2] as usize;
        if i0 * 3 + 2 >= mesh.positions.len()
            || i1 * 3 + 2 >= mesh.positions.len()
            || i2 * 3 + 2 >= mesh.positions.len()
        {
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

        let normal = normalize(cross(sub(v1, v0), sub(v2, v0)));
        append_f32_triplet(&mut data, normal);
        append_f32_triplet(&mut data, v0);
        append_f32_triplet(&mut data, v1);
        append_f32_triplet(&mut data, v2);
        data.extend_from_slice(&0u16.to_le_bytes());
    }

    fs::write(path, data)?;
    Ok(())
}

pub fn load_binary_stl(path: &Path) -> Result<MeshData> {
    let bytes = fs::read(path)?;
    if bytes.len() < 84 {
        return Err(anyhow!("STL too small: {}", path.display()));
    }

    let tri_count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    let expected_len = 84 + tri_count * 50;
    if bytes.len() != expected_len {
        return Err(anyhow!(
            "Only binary STL with exact length is supported (got {}, expected {}) for {}",
            bytes.len(),
            expected_len,
            path.display()
        ));
    }

    let mut positions = Vec::with_capacity(tri_count * 9);
    let mut normals = Vec::with_capacity(tri_count * 9);
    let mut colors = Vec::with_capacity(tri_count * 9);
    let mut indices = Vec::with_capacity(tri_count * 3);

    for tri in 0..tri_count {
        let base = 84 + tri * 50;
        let nx = read_f32_at(&bytes, base);
        let ny = read_f32_at(&bytes, base + 4);
        let nz = read_f32_at(&bytes, base + 8);

        for v in 0..3 {
            let vbase = base + 12 + v * 12;
            let x = read_f32_at(&bytes, vbase);
            let y = read_f32_at(&bytes, vbase + 4);
            let z = read_f32_at(&bytes, vbase + 8);
            positions.extend_from_slice(&[x, y, z]);
            normals.extend_from_slice(&[nx, ny, nz]);
            colors.extend_from_slice(&[1.0, 1.0, 1.0]);
            indices.push((tri * 3 + v) as u32);
        }
    }

    Ok(MeshData {
        positions,
        normals,
        colors,
        indices,
    })
}

pub fn compare_meshes(
    mesh_a: &MeshData,
    mesh_b: &MeshData,
    config: OracleConfig,
) -> Result<OracleReport> {
    let prepared_a = prepare_mesh(mesh_a)?;
    let prepared_b = prepare_mesh(mesh_b)?;

    let sample_points_a = sample_surface_points(&prepared_a, config.surface_samples);
    let sample_points_b = sample_surface_points(&prepared_b, config.surface_samples);

    let mut distances_a_to_b = Vec::with_capacity(sample_points_a.len());
    let mut distances_b_to_a = Vec::with_capacity(sample_points_b.len());

    for point in &sample_points_a {
        distances_a_to_b.push(nearest_distance_to_mesh(*point, &prepared_b));
    }
    for point in &sample_points_b {
        distances_b_to_a.push(nearest_distance_to_mesh(*point, &prepared_a));
    }

    let surface_a_to_b = summarize_distances(&distances_a_to_b);
    let surface_b_to_a = summarize_distances(&distances_b_to_a);
    let symmetric_max_mm = surface_a_to_b.max_mm.max(surface_b_to_a.max_mm);
    let symmetric_p95_mm = surface_a_to_b.p95_mm.max(surface_b_to_a.p95_mm);

    let bounds_union = union_bounds(prepared_a.bounds, prepared_b.bounds);
    let bbox_volume = bounds_volume(bounds_union) as f64;

    let mut disagreements = 0usize;
    let mut disagreements_outside_band = 0usize;
    let mut inside_a = 0usize;
    let mut inside_b = 0usize;

    for i in 0..config.volume_samples {
        let p = sample_point_in_bounds(bounds_union, i);
        let in_a = point_inside_mesh(p, &prepared_a);
        let in_b = point_inside_mesh(p, &prepared_b);

        if in_a {
            inside_a += 1;
        }
        if in_b {
            inside_b += 1;
        }

        if in_a != in_b {
            disagreements += 1;

            let da = nearest_distance_to_mesh(p, &prepared_a);
            let db = nearest_distance_to_mesh(p, &prepared_b);
            if da.min(db) > config.boundary_band_mm {
                disagreements_outside_band += 1;
            }
        }
    }

    let disagreement_rate = disagreements as f64 / config.volume_samples as f64;
    let disagreement_rate_outside_boundary_band =
        disagreements_outside_band as f64 / config.volume_samples as f64;

    let volume_a = bbox_volume * (inside_a as f64 / config.volume_samples as f64);
    let volume_b = bbox_volume * (inside_b as f64 / config.volume_samples as f64);
    let abs_delta = (volume_a - volume_b).abs();
    let rel_delta = if volume_a > 1e-9 {
        abs_delta / volume_a.abs().max(1e-9)
    } else {
        0.0
    };

    let bounds = compute_bounds_metrics(prepared_a.bounds, prepared_b.bounds);

    let inside_outside = InsideOutsideMetrics {
        total_samples: config.volume_samples,
        disagreements,
        disagreement_rate,
        disagreements_outside_boundary_band: disagreements_outside_band,
        disagreement_rate_outside_boundary_band,
    };

    let volume = VolumeMetrics {
        bbox_volume_mm3: bbox_volume,
        volume_a_mm3: volume_a,
        volume_b_mm3: volume_b,
        abs_delta_mm3: abs_delta,
        rel_delta,
    };

    let mut failed_checks = Vec::new();
    if symmetric_p95_mm > config.max_surface_p95_mm {
        failed_checks.push(format!(
            "symmetric_p95_mm {:.6} > tolerance {:.6}",
            symmetric_p95_mm, config.max_surface_p95_mm
        ));
    }
    if disagreement_rate_outside_boundary_band > config.max_inside_disagreement_rate {
        failed_checks.push(format!(
            "inside_disagreement_outside_band {:.6} > tolerance {:.6}",
            disagreement_rate_outside_boundary_band, config.max_inside_disagreement_rate
        ));
    }

    Ok(OracleReport {
        config,
        surface_a_to_b,
        surface_b_to_a,
        symmetric_max_mm,
        symmetric_p95_mm,
        inside_outside,
        volume,
        bounds,
        pass: failed_checks.is_empty(),
        failed_checks,
    })
}

fn prepare_mesh(mesh: &MeshData) -> Result<PreparedMesh> {
    let mut triangles = Vec::new();
    for tri in 0..(mesh.indices.len() / 3) {
        let i0 = mesh.indices[tri * 3] as usize;
        let i1 = mesh.indices[tri * 3 + 1] as usize;
        let i2 = mesh.indices[tri * 3 + 2] as usize;
        if i0 * 3 + 2 >= mesh.positions.len()
            || i1 * 3 + 2 >= mesh.positions.len()
            || i2 * 3 + 2 >= mesh.positions.len()
        {
            continue;
        }

        let a = [
            mesh.positions[i0 * 3],
            mesh.positions[i0 * 3 + 1],
            mesh.positions[i0 * 3 + 2],
        ];
        let b = [
            mesh.positions[i1 * 3],
            mesh.positions[i1 * 3 + 1],
            mesh.positions[i1 * 3 + 2],
        ];
        let c = [
            mesh.positions[i2 * 3],
            mesh.positions[i2 * 3 + 1],
            mesh.positions[i2 * 3 + 2],
        ];
        let area = triangle_area(a, b, c);
        if area > 1e-12 {
            triangles.push(Triangle { a, b, c, area });
        }
    }

    if triangles.is_empty() {
        return Err(anyhow!("mesh has no valid triangles"));
    }

    let bounds = mesh_bounds(mesh)?;
    Ok(PreparedMesh { triangles, bounds })
}

fn mesh_bounds(mesh: &MeshData) -> Result<Bounds> {
    if mesh.positions.is_empty() {
        return Err(anyhow!("mesh has no positions"));
    }

    let mut min = [f32::MAX, f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN, f32::MIN];

    for i in 0..(mesh.positions.len() / 3) {
        let x = mesh.positions[i * 3];
        let y = mesh.positions[i * 3 + 1];
        let z = mesh.positions[i * 3 + 2];
        min[0] = min[0].min(x);
        min[1] = min[1].min(y);
        min[2] = min[2].min(z);
        max[0] = max[0].max(x);
        max[1] = max[1].max(y);
        max[2] = max[2].max(z);
    }

    Ok(Bounds { min, max })
}

fn sample_surface_points(mesh: &PreparedMesh, sample_count: usize) -> Vec<[f32; 3]> {
    let count = sample_count.max(1);
    let mut cdf = Vec::with_capacity(mesh.triangles.len());
    let mut total_area = 0.0f64;
    for tri in &mesh.triangles {
        total_area += tri.area as f64;
        cdf.push(total_area);
    }

    let mut points = Vec::with_capacity(count);
    for i in 0..count {
        let sel = halton(i + 1, 2) * total_area;
        let tri_idx = cdf
            .partition_point(|v| *v < sel)
            .min(mesh.triangles.len() - 1);
        let tri = mesh.triangles[tri_idx];

        let u = halton(i + 1, 3);
        let v = halton(i + 1, 5);
        points.push(sample_point_on_triangle(tri, u, v));
    }
    points
}

fn sample_point_on_triangle(tri: Triangle, u: f64, v: f64) -> [f32; 3] {
    let su = u.sqrt() as f32;
    let b0 = 1.0 - su;
    let b1 = su * (1.0 - v as f32);
    let b2 = su * (v as f32);
    [
        tri.a[0] * b0 + tri.b[0] * b1 + tri.c[0] * b2,
        tri.a[1] * b0 + tri.b[1] * b1 + tri.c[1] * b2,
        tri.a[2] * b0 + tri.b[2] * b1 + tri.c[2] * b2,
    ]
}

fn sample_point_in_bounds(bounds: Bounds, i: usize) -> [f32; 3] {
    let x = lerp(bounds.min[0], bounds.max[0], halton(i + 1, 2) as f32);
    let y = lerp(bounds.min[1], bounds.max[1], halton(i + 1, 3) as f32);
    let z = lerp(bounds.min[2], bounds.max[2], halton(i + 1, 5) as f32);
    [x, y, z]
}

fn nearest_distance_to_mesh(point: [f32; 3], mesh: &PreparedMesh) -> f32 {
    let mut best = f32::MAX;
    for tri in &mesh.triangles {
        let d = point_triangle_distance(point, tri.a, tri.b, tri.c);
        best = best.min(d);
    }
    best
}

fn summarize_distances(distances: &[f32]) -> SurfaceDistanceMetrics {
    if distances.is_empty() {
        return SurfaceDistanceMetrics {
            max_mm: 0.0,
            p95_mm: 0.0,
            mean_mm: 0.0,
        };
    }

    let mut sorted = distances.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let max_mm = *sorted.last().unwrap_or(&0.0);
    let p95_mm = percentile(&sorted, 0.95);
    let mean_mm = sorted.iter().copied().sum::<f32>() / sorted.len() as f32;
    SurfaceDistanceMetrics {
        max_mm,
        p95_mm,
        mean_mm,
    }
}

fn percentile(sorted: &[f32], p: f64) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    let idx = ((n as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(n - 1)]
}

fn point_inside_mesh(point: [f32; 3], mesh: &PreparedMesh) -> bool {
    let dirs = [
        [1.0, 0.271, 0.113],
        [0.317, 1.0, 0.191],
        [0.137, 0.229, 1.0],
    ];
    let mut inside_count = 0usize;
    for dir in dirs {
        let intersections = ray_intersection_count(point, dir, mesh);
        if intersections % 2 == 1 {
            inside_count += 1;
        }
    }
    inside_count >= 2
}

fn ray_intersection_count(origin: [f32; 3], direction: [f32; 3], mesh: &PreparedMesh) -> usize {
    let dir = normalize(direction);
    let mut count = 0usize;
    for tri in &mesh.triangles {
        if ray_triangle_intersection(origin, dir, tri.a, tri.b, tri.c).is_some() {
            count += 1;
        }
    }
    count
}

fn ray_triangle_intersection(
    origin: [f32; 3],
    direction: [f32; 3],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
) -> Option<f32> {
    let epsilon = 1e-7f32;
    let edge1 = sub(v1, v0);
    let edge2 = sub(v2, v0);
    let h = cross(direction, edge2);
    let a = dot(edge1, h);
    if a.abs() < epsilon {
        return None;
    }

    let f = 1.0 / a;
    let s = sub(origin, v0);
    let u = f * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = cross(s, edge1);
    let v = f * dot(direction, q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * dot(edge2, q);
    if t > epsilon {
        Some(t)
    } else {
        None
    }
}

fn point_triangle_distance(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);

    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return length(sub(p, a));
    }

    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return length(sub(p, b));
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        let proj = add(a, mul(ab, v));
        return length(sub(p, proj));
    }

    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return length(sub(p, c));
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        let proj = add(a, mul(ac, w));
        return length(sub(p, proj));
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let proj = add(b, mul(sub(c, b), w));
        return length(sub(p, proj));
    }

    let n = normalize(cross(ab, ac));
    dot(sub(p, a), n).abs()
}

fn compute_bounds_metrics(a: Bounds, b: Bounds) -> BoundsMetrics {
    let min_delta = [
        (a.min[0] - b.min[0]).abs(),
        (a.min[1] - b.min[1]).abs(),
        (a.min[2] - b.min[2]).abs(),
    ];
    let max_delta = [
        (a.max[0] - b.max[0]).abs(),
        (a.max[1] - b.max[1]).abs(),
        (a.max[2] - b.max[2]).abs(),
    ];
    let max_abs_delta_mm = min_delta
        .iter()
        .chain(max_delta.iter())
        .copied()
        .fold(0.0f32, f32::max);

    BoundsMetrics {
        min_delta_mm: min_delta,
        max_delta_mm: max_delta,
        max_abs_delta_mm,
    }
}

fn union_bounds(a: Bounds, b: Bounds) -> Bounds {
    Bounds {
        min: [
            a.min[0].min(b.min[0]),
            a.min[1].min(b.min[1]),
            a.min[2].min(b.min[2]),
        ],
        max: [
            a.max[0].max(b.max[0]),
            a.max[1].max(b.max[1]),
            a.max[2].max(b.max[2]),
        ],
    }
}

fn bounds_volume(bounds: Bounds) -> f32 {
    let dx = (bounds.max[0] - bounds.min[0]).max(0.0);
    let dy = (bounds.max[1] - bounds.min[1]).max(0.0);
    let dz = (bounds.max[2] - bounds.min[2]).max(0.0);
    dx * dy * dz
}

fn triangle_area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    0.5 * length(cross(sub(b, a), sub(c, a)))
}

fn halton(mut index: usize, base: usize) -> f64 {
    let mut f = 1.0f64;
    let mut r = 0.0f64;
    while index > 0 {
        f /= base as f64;
        r += f * (index % base) as f64;
        index /= base;
    }
    r
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn mul(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = length(v);
    if len <= 1e-12 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn append_f32_triplet(out: &mut Vec<u8>, v: [f32; 3]) {
    out.extend_from_slice(&v[0].to_le_bytes());
    out.extend_from_slice(&v[1].to_le_bytes());
    out.extend_from_slice(&v[2].to_le_bytes());
}

fn read_f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
