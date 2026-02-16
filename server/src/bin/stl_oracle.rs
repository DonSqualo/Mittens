use anyhow::{anyhow, Result};
use clap::Parser;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

type V3 = [f32; 3];

#[derive(Parser, Debug)]
#[command(name = "stl_oracle")]
#[command(about = "Fast STL conformance oracle (surface distance)")]
struct Cli {
    #[arg(long)]
    truth: PathBuf,
    #[arg(long)]
    candidate: PathBuf,
    #[arg(long, default_value_t = 1200)]
    samples: usize,
    #[arg(long, default_value_t = 0.10)]
    max_p95_mm: f32,
    #[arg(long, default_value_t = 0.60)]
    max_max_mm: f32,
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Copy)]
struct Tri {
    a: V3,
    b: V3,
    c: V3,
    min: V3,
    max: V3,
}

struct Mesh {
    tris: Vec<Tri>,
    cdf: Vec<f64>,
    total_area: f64,
    bounds: [V3; 2],
    volume_mm3: f64,
}

struct Grid {
    origin: V3,
    inv_cell: f32,
    cell: f32,
    dims: [i32; 3],
    cells: HashMap<(i32, i32, i32), Vec<usize>>,
}

#[derive(Clone, Copy, Serialize)]
struct Dist {
    max_mm: f32,
    p95_mm: f32,
    mean_mm: f32,
}

#[derive(Serialize)]
struct Report {
    truth_file: String,
    candidate_file: String,
    truth_triangles: usize,
    candidate_triangles: usize,
    samples_per_direction: usize,
    truth_bounds: [V3; 2],
    candidate_bounds: [V3; 2],
    truth_volume_mm3: f64,
    candidate_volume_mm3: f64,
    volume_abs_delta_mm3: f64,
    volume_rel_delta: f64,
    truth_to_candidate: Dist,
    candidate_to_truth: Dist,
    symmetric_p95_mm: f32,
    symmetric_max_mm: f32,
    max_p95_mm: f32,
    max_max_mm: f32,
    pass: bool,
    failed_checks: Vec<String>,
    load_ms: u128,
    prepare_ms: u128,
    compare_ms: u128,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let t0 = Instant::now();
    let truth_raw = load_binary_stl(&cli.truth)?;
    let cand_raw = load_binary_stl(&cli.candidate)?;
    let load_ms = t0.elapsed().as_millis();

    let t1 = Instant::now();
    let truth = prepare(&truth_raw)?;
    let cand = prepare(&cand_raw)?;
    let truth_grid = Grid::build(&truth);
    let cand_grid = Grid::build(&cand);
    let prepare_ms = t1.elapsed().as_millis();

    let t2 = Instant::now();
    let n = cli.samples.max(1);
    let a_to_b = sampled_distances(&truth, &cand, &cand_grid, n);
    let b_to_a = sampled_distances(&cand, &truth, &truth_grid, n);
    let compare_ms = t2.elapsed().as_millis();

    let s_ab = summarize(&a_to_b);
    let s_ba = summarize(&b_to_a);
    let sym_p95 = s_ab.p95_mm.max(s_ba.p95_mm);
    let sym_max = s_ab.max_mm.max(s_ba.max_mm);

    let vol_abs = (truth.volume_mm3 - cand.volume_mm3).abs();
    let vol_rel = if truth.volume_mm3.abs() > 1e-9 {
        vol_abs / truth.volume_mm3.abs()
    } else {
        0.0
    };

    let mut failed_checks = Vec::new();
    if sym_p95 > cli.max_p95_mm {
        failed_checks.push(format!("symmetric_p95_mm {:.6} > {:.6}", sym_p95, cli.max_p95_mm));
    }
    if sym_max > cli.max_max_mm {
        failed_checks.push(format!("symmetric_max_mm {:.6} > {:.6}", sym_max, cli.max_max_mm));
    }

    let report = Report {
        truth_file: cli.truth.display().to_string(),
        candidate_file: cli.candidate.display().to_string(),
        truth_triangles: truth.tris.len(),
        candidate_triangles: cand.tris.len(),
        samples_per_direction: n,
        truth_bounds: truth.bounds,
        candidate_bounds: cand.bounds,
        truth_volume_mm3: truth.volume_mm3,
        candidate_volume_mm3: cand.volume_mm3,
        volume_abs_delta_mm3: vol_abs,
        volume_rel_delta: vol_rel,
        truth_to_candidate: s_ab,
        candidate_to_truth: s_ba,
        symmetric_p95_mm: sym_p95,
        symmetric_max_mm: sym_max,
        max_p95_mm: cli.max_p95_mm,
        max_max_mm: cli.max_max_mm,
        pass: failed_checks.is_empty(),
        failed_checks,
        load_ms,
        prepare_ms,
        compare_ms,
    };

    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = cli.out {
        fs::write(path, &json)?;
    }
    println!("{}", json);

    if report.pass {
        Ok(())
    } else {
        Err(anyhow!("conformance failed"))
    }
}

struct RawMesh {
    positions: Vec<V3>,
    tris: Vec<[u32; 3]>,
}

fn load_binary_stl(path: &PathBuf) -> Result<RawMesh> {
    let bytes = fs::read(path)?;
    if bytes.len() < 84 {
        return Err(anyhow!("stl too small: {}", path.display()));
    }
    let tri_count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    let expected = 84 + tri_count * 50;
    if bytes.len() != expected {
        return Err(anyhow!(
            "only exact binary STL supported (got {}, expected {}) for {}",
            bytes.len(),
            expected,
            path.display()
        ));
    }

    let mut positions = Vec::with_capacity(tri_count * 3);
    let mut tris = Vec::with_capacity(tri_count);
    for i in 0..tri_count {
        let base = 84 + i * 50;
        let mut idx = [0_u32; 3];
        for (v, out_idx) in idx.iter_mut().enumerate() {
            let off = base + 12 + v * 12;
            let x = f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
            let y =
                f32::from_le_bytes([bytes[off + 4], bytes[off + 5], bytes[off + 6], bytes[off + 7]]);
            let z =
                f32::from_le_bytes([bytes[off + 8], bytes[off + 9], bytes[off + 10], bytes[off + 11]]);
            positions.push([x, y, z]);
            *out_idx = (i * 3 + v) as u32;
        }
        tris.push(idx);
    }
    Ok(RawMesh { positions, tris })
}

fn prepare(raw: &RawMesh) -> Result<Mesh> {
    if raw.tris.is_empty() {
        return Err(anyhow!("mesh has no triangles"));
    }

    let mut bounds_min = [f32::MAX; 3];
    let mut bounds_max = [f32::MIN; 3];
    let mut tris = Vec::with_capacity(raw.tris.len());
    let mut cdf = Vec::with_capacity(raw.tris.len());
    let mut total_area = 0.0_f64;
    let mut signed_volume = 0.0_f64;

    for t in &raw.tris {
        let a = raw.positions[t[0] as usize];
        let b = raw.positions[t[1] as usize];
        let c = raw.positions[t[2] as usize];

        bounds_min = vmin(bounds_min, vmin(a, vmin(b, c)));
        bounds_max = vmax(bounds_max, vmax(a, vmax(b, c)));

        let ab = sub(b, a);
        let ac = sub(c, a);
        let area = 0.5 * len(cross(ab, ac));
        if area <= 1e-12 {
            continue;
        }

        tris.push(Tri {
            a,
            b,
            c,
            min: vmin(a, vmin(b, c)),
            max: vmax(a, vmax(b, c)),
        });
        total_area += area as f64;
        cdf.push(total_area);
        signed_volume += dot(a, cross(b, c)) as f64 / 6.0;
    }

    if tris.is_empty() {
        return Err(anyhow!("mesh has no non-degenerate triangles"));
    }

    Ok(Mesh {
        tris,
        cdf,
        total_area,
        bounds: [bounds_min, bounds_max],
        volume_mm3: signed_volume.abs(),
    })
}

impl Grid {
    fn build(mesh: &Mesh) -> Self {
        let min = mesh.bounds[0];
        let max = mesh.bounds[1];
        let ext = sub(max, min);
        let max_ext = ext[0].max(ext[1]).max(ext[2]).max(1e-3);
        let cells_per_axis = (mesh.tris.len() as f32).cbrt().max(8.0);
        let cell = (max_ext / cells_per_axis).max(1e-4);
        let inv_cell = 1.0 / cell;
        let dims = [
            ((ext[0] * inv_cell).ceil() as i32).max(1),
            ((ext[1] * inv_cell).ceil() as i32).max(1),
            ((ext[2] * inv_cell).ceil() as i32).max(1),
        ];

        let mut cells: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
        for (i, tri) in mesh.tris.iter().enumerate() {
            let lo = to_cell(tri.min, min, inv_cell, dims);
            let hi = to_cell(tri.max, min, inv_cell, dims);
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    for z in lo.2..=hi.2 {
                        cells.entry((x, y, z)).or_default().push(i);
                    }
                }
            }
        }

        Self {
            origin: min,
            inv_cell,
            cell,
            dims,
            cells,
        }
    }

    fn nearest(&self, mesh: &Mesh, p: V3, marks: &mut [u32], mark: u32) -> f32 {
        let base = to_cell(p, self.origin, self.inv_cell, self.dims);
        let max_r = self.dims[0].max(self.dims[1]).max(self.dims[2]).max(1);
        let mut best = f32::MAX;

        for r in 0..=max_r {
            for (cx, cy, cz) in shell(base, r, self.dims) {
                if let Some(list) = self.cells.get(&(cx, cy, cz)) {
                    for &ti in list {
                        if marks[ti] == mark {
                            continue;
                        }
                        marks[ti] = mark;
                        let tri = mesh.tris[ti];
                        best = best.min(point_tri_dist(p, tri.a, tri.b, tri.c));
                    }
                }
            }
            if best.is_finite() {
                let lb = ((r as f32) - 0.5).max(0.0) * self.cell;
                if best <= lb {
                    break;
                }
            }
        }

        if best.is_finite() {
            best
        } else {
            mesh.tris
                .iter()
                .map(|t| point_tri_dist(p, t.a, t.b, t.c))
                .fold(f32::MAX, f32::min)
        }
    }
}

fn sampled_distances(source: &Mesh, target: &Mesh, target_grid: &Grid, n: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(n);
    let mut marks = vec![0_u32; target.tris.len()];
    let mut mark = 1_u32;

    for i in 0..n {
        let pick = halton(i + 1, 2) * source.total_area;
        let ti = source
            .cdf
            .partition_point(|a| *a < pick)
            .min(source.tris.len() - 1);
        let tri = source.tris[ti];
        let p = sample_tri(tri, halton(i + 1, 3), halton(i + 1, 5));
        out.push(target_grid.nearest(target, p, &mut marks, mark));

        mark = mark.wrapping_add(1);
        if mark == 0 {
            marks.fill(0);
            mark = 1;
        }
    }

    out
}

fn summarize(d: &[f32]) -> Dist {
    if d.is_empty() {
        return Dist {
            max_mm: 0.0,
            p95_mm: 0.0,
            mean_mm: 0.0,
        };
    }
    let mut s = d.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let n = s.len();
    let i95 = (((n as f64 - 1.0) * 0.95).round() as usize).min(n - 1);
    Dist {
        max_mm: *s.last().unwrap_or(&0.0),
        p95_mm: s[i95],
        mean_mm: s.iter().copied().sum::<f32>() / n as f32,
    }
}

fn to_cell(p: V3, origin: V3, inv_cell: f32, dims: [i32; 3]) -> (i32, i32, i32) {
    let idx = |v: f32, o: f32, d: i32| (((v - o) * inv_cell).floor() as i32).clamp(0, d - 1);
    (
        idx(p[0], origin[0], dims[0]),
        idx(p[1], origin[1], dims[1]),
        idx(p[2], origin[2], dims[2]),
    )
}

fn shell(base: (i32, i32, i32), r: i32, dims: [i32; 3]) -> Vec<(i32, i32, i32)> {
    if r == 0 {
        return vec![base];
    }
    let (bx, by, bz) = base;
    let (minx, maxx) = ((bx - r).max(0), (bx + r).min(dims[0] - 1));
    let (miny, maxy) = ((by - r).max(0), (by + r).min(dims[1] - 1));
    let (minz, maxz) = ((bz - r).max(0), (bz + r).min(dims[2] - 1));

    let mut out = Vec::new();
    for x in minx..=maxx {
        for y in miny..=maxy {
            for z in minz..=maxz {
                if x == minx || x == maxx || y == miny || y == maxy || z == minz || z == maxz {
                    out.push((x, y, z));
                }
            }
        }
    }
    out
}

fn point_tri_dist(p: V3, a: V3, b: V3, c: V3) -> f32 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return len(sub(p, a));
    }

    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return len(sub(p, b));
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return len(sub(p, add(a, mul(ab, d1 / (d1 - d3)))));
    }

    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return len(sub(p, c));
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return len(sub(p, add(a, mul(ac, d2 / (d2 - d6)))));
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return len(sub(p, add(b, mul(sub(c, b), w))));
    }

    dot(sub(p, a), norm(cross(ab, ac))).abs()
}

fn sample_tri(t: Tri, u: f64, v: f64) -> V3 {
    let su = (u as f32).sqrt();
    let b0 = 1.0 - su;
    let b1 = su * (1.0 - v as f32);
    let b2 = su * (v as f32);
    [
        t.a[0] * b0 + t.b[0] * b1 + t.c[0] * b2,
        t.a[1] * b0 + t.b[1] * b1 + t.c[1] * b2,
        t.a[2] * b0 + t.b[2] * b1 + t.c[2] * b2,
    ]
}

fn halton(mut i: usize, base: usize) -> f64 {
    let mut out = 0.0;
    let mut f = 1.0;
    while i > 0 {
        f /= base as f64;
        out += f * (i % base) as f64;
        i /= base;
    }
    out
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(v: V3) -> f32 {
    (dot(v, v)).sqrt()
}
fn norm(v: V3) -> V3 {
    let l = len(v);
    if l <= 1e-12 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}
fn vmin(a: V3, b: V3) -> V3 {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])]
}
fn vmax(a: V3, b: V3) -> V3 {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}
