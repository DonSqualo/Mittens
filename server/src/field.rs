//! Magnetic field computation using Biot-Savart law
//!
//! Computes B-field from circular current loops (coils)

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaneType {
    XZ = 0,  // Plane at Y=offset
    XY = 1,  // Plane at Z=offset
    YZ = 2,  // Plane at X=offset
}

const MU0: f64 = 4.0 * PI * 1e-7; // Permeability of free space (H/m)

/// A circular current loop
#[derive(Clone)]
pub struct CurrentLoop {
    pub center: [f64; 3],    // Center position (mm)
    pub radius: f64,         // Radius (mm)
    pub normal: [f64; 3],    // Normal direction (unit vector)
    pub ampere_turns: f64,   // Current × turns (A·turns)
}

/// Colormap selection for field visualization
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Colormap {
    #[default]
    Jet = 0,
    Viridis = 1,
    Plasma = 2,
    PressureBlueRed = 3,  // For acoustic visualization: Blue (low) -> Red (high)
}

impl Colormap {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "viridis" => Colormap::Viridis,
            "plasma" => Colormap::Plasma,
            _ => Colormap::Jet,
        }
    }
}

/// Magnetic field data for visualization
pub struct FieldData {
    // 2D slice data
    pub plane_type: PlaneType,
    pub colormap: Colormap,
    pub slice_width: usize,
    pub slice_height: usize,
    pub slice_bounds: [f64; 4],  // Bounds in mm: [axis1_min, axis1_max, axis2_min, axis2_max]
    pub slice_offset: f64,       // Offset along normal axis (mm)
    pub slice_bx: Vec<f32>,      // B component along first axis (T)
    pub slice_bz: Vec<f32>,      // B component along second axis (T)
    pub slice_magnitude: Vec<f32>, // |B| (T)

    // 3D arrow field
    pub arrows_positions: Vec<f32>,  // x, y, z positions
    pub arrows_vectors: Vec<f32>,    // Bx, By, Bz vectors (normalized for display)
    pub arrows_magnitudes: Vec<f32>, // |B| for coloring

    // 1D line data (along Z axis)
    pub line_z: Vec<f32>,        // Z positions (mm)
    pub line_bz: Vec<f32>,       // Bz values (T)
}

impl FieldData {
    pub fn to_binary(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // Header: type marker
        data.extend_from_slice(b"FIELD\0\0\0");

        // 2D slice dimensions
        data.extend_from_slice(&(self.slice_width as u32).to_le_bytes());
        data.extend_from_slice(&(self.slice_height as u32).to_le_bytes());

        // 2D slice bounds
        for &b in &self.slice_bounds {
            data.extend_from_slice(&(b as f32).to_le_bytes());
        }

        // Plane type (u8), offset (f32), colormap (u8)
        data.push(self.plane_type as u8);
        data.extend_from_slice(&(self.slice_offset as f32).to_le_bytes());
        data.push(self.colormap as u8);

        // 2D slice data
        for &v in &self.slice_bx {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for &v in &self.slice_bz {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for &v in &self.slice_magnitude {
            data.extend_from_slice(&v.to_le_bytes());
        }

        // 3D arrows count
        let num_arrows = self.arrows_positions.len() / 3;
        data.extend_from_slice(&(num_arrows as u32).to_le_bytes());

        // 3D arrow data
        for &v in &self.arrows_positions {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for &v in &self.arrows_vectors {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for &v in &self.arrows_magnitudes {
            data.extend_from_slice(&v.to_le_bytes());
        }

        // 1D line count
        data.extend_from_slice(&(self.line_z.len() as u32).to_le_bytes());

        // 1D line data
        for &v in &self.line_z {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for &v in &self.line_bz {
            data.extend_from_slice(&v.to_le_bytes());
        }

        data
    }
}

/// Compute B-field from a single current loop at a point using Biot-Savart law
fn biot_savart_loop(loop_: &CurrentLoop, point: [f64; 3], num_segments: usize) -> [f64; 3] {
    let mut b = [0.0, 0.0, 0.0];

    // Convert mm to m for SI units
    let r_m = loop_.radius * 1e-3;
    let cx = loop_.center[0] * 1e-3;
    let cy = loop_.center[1] * 1e-3;
    let cz = loop_.center[2] * 1e-3;
    let px = point[0] * 1e-3;
    let py = point[1] * 1e-3;
    let pz = point[2] * 1e-3;

    // Build orthonormal basis (u, v, n) where n is the coil normal
    let nx = loop_.normal[0];
    let ny = loop_.normal[1];
    let nz = loop_.normal[2];

    // Find a reference vector not parallel to n for cross product
    let (ref_x, ref_y, ref_z) = if nx.abs() < 0.9 {
        (1.0, 0.0, 0.0)
    } else {
        (0.0, 1.0, 0.0)
    };

    // u = n × ref (then normalize)
    let ux_raw = ny * ref_z - nz * ref_y;
    let uy_raw = nz * ref_x - nx * ref_z;
    let uz_raw = nx * ref_y - ny * ref_x;
    let u_mag = (ux_raw * ux_raw + uy_raw * uy_raw + uz_raw * uz_raw).sqrt();
    let ux = ux_raw / u_mag;
    let uy = uy_raw / u_mag;
    let uz = uz_raw / u_mag;

    // v = n × u
    let vx = ny * uz - nz * uy;
    let vy = nz * ux - nx * uz;
    let vz = nx * uy - ny * ux;

    // Wire position: center + R * (u * cos(θ) + v * sin(θ))
    // dl vector: R * dθ * (-u * sin(θ) + v * cos(θ))

    let dtheta = 2.0 * PI / num_segments as f64;

    for i in 0..num_segments {
        let theta = i as f64 * dtheta;
        let theta_mid = theta + dtheta / 2.0;

        let cos_t = theta_mid.cos();
        let sin_t = theta_mid.sin();

        // Wire element position
        let wx = cx + r_m * (ux * cos_t + vx * sin_t);
        let wy = cy + r_m * (uy * cos_t + vy * sin_t);
        let wz = cz + r_m * (uz * cos_t + vz * sin_t);

        // dl vector (tangent to loop)
        let dlx = r_m * dtheta * (-ux * sin_t + vx * cos_t);
        let dly = r_m * dtheta * (-uy * sin_t + vy * cos_t);
        let dlz = r_m * dtheta * (-uz * sin_t + vz * cos_t);

        // Vector from wire element to field point
        let rx = px - wx;
        let ry = py - wy;
        let rz = pz - wz;
        let r_mag = (rx * rx + ry * ry + rz * rz).sqrt();

        if r_mag < 1e-10 {
            continue;
        }

        // dl × r
        let r3 = r_mag * r_mag * r_mag;
        let cross_x = dly * rz - dlz * ry;
        let cross_y = dlz * rx - dlx * rz;
        let cross_z = dlx * ry - dly * rx;

        // dB = (mu_0/4pi) * I * (dl x r) / r^3
        let factor = MU0 / (4.0 * PI) * loop_.ampere_turns / r3;

        b[0] += factor * cross_x;
        b[1] += factor * cross_y;
        b[2] += factor * cross_z;
    }

    b
}

/// Compute total B-field from multiple loops at a point
fn compute_field(loops: &[CurrentLoop], point: [f64; 3]) -> [f64; 3] {
    let mut b = [0.0, 0.0, 0.0];

    for loop_ in loops {
        let b_loop = biot_savart_loop(loop_, point, 64);
        b[0] += b_loop[0];
        b[1] += b_loop[1];
        b[2] += b_loop[2];
    }

    b
}

/// Measurement result from a GaussMeter or similar point probe
#[derive(Clone, Debug)]
pub struct PointMeasurement {
    pub position: [f64; 3],
    pub value: [f64; 3],  // Bx, By, Bz in Tesla
    pub magnitude: f64,   // |B| in Tesla
    pub label: String,
}

impl PointMeasurement {
    pub fn to_binary(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(40);
        data.extend_from_slice(b"MEASURE\0");
        for &p in &self.position {
            data.extend_from_slice(&(p as f32).to_le_bytes());
        }
        for &v in &self.value {
            data.extend_from_slice(&(v as f32).to_le_bytes());
        }
        data.extend_from_slice(&(self.magnitude as f32).to_le_bytes());
        let label_bytes = self.label.as_bytes();
        data.extend_from_slice(&(label_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(label_bytes);
        data
    }
}

#[derive(Clone, Debug)]
pub struct ProbeStatistics {
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub std: f32,
}

#[derive(Clone, Debug)]
pub struct LineMeasurement {
    pub name: String,
    pub start: [f64; 3],
    pub stop: [f64; 3],
    pub positions: Vec<f32>,
    pub values: Vec<f32>,
    pub magnitudes: Vec<f32>,
    pub statistics: Option<ProbeStatistics>,
}

impl LineMeasurement {
    pub fn to_binary(&self) -> Vec<u8> {
        let num_points = self.magnitudes.len();
        let mut data = Vec::with_capacity(64 + num_points * 28);
        data.extend_from_slice(b"LNPROBE\0");
        data.extend_from_slice(&(num_points as u32).to_le_bytes());
        for &v in &self.start {
            data.extend_from_slice(&(v as f32).to_le_bytes());
        }
        for &v in &self.stop {
            data.extend_from_slice(&(v as f32).to_le_bytes());
        }
        for &v in &self.positions {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for &v in &self.values {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for &v in &self.magnitudes {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let name_bytes = self.name.as_bytes();
        data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(name_bytes);
        match &self.statistics {
            Some(stats) => {
                data.push(1);
                data.extend_from_slice(&stats.min.to_le_bytes());
                data.extend_from_slice(&stats.max.to_le_bytes());
                data.extend_from_slice(&stats.mean.to_le_bytes());
                data.extend_from_slice(&stats.std.to_le_bytes());
            }
            None => {
                data.push(0);
            }
        }
        data
    }
}

/// Compute magnetic field at a specific point for GaussMeter measurements
/// Uses same Helmholtz coil configuration as visualization
pub fn compute_point_field(
    coil_inner_r: f64,
    coil_outer_r: f64,
    coil_width: f64,
    gap: f64,
    ampere_turns: f64,
    num_layers: usize,
    point: [f64; 3],
) -> [f64; 3] {
    let mut loops = Vec::new();

    let coil_z = gap / 2.0 + coil_width / 2.0;
    let dr = (coil_outer_r - coil_inner_r) / num_layers as f64;
    let at_per_layer = ampere_turns / num_layers as f64;

    for layer in 0..num_layers {
        let r = coil_inner_r + (layer as f64 + 0.5) * dr;

        loops.push(CurrentLoop {
            center: [0.0, 0.0, coil_z],
            radius: r,
            normal: [0.0, 0.0, 1.0],
            ampere_turns: at_per_layer,
        });

        loops.push(CurrentLoop {
            center: [0.0, 0.0, -coil_z],
            radius: r,
            normal: [0.0, 0.0, 1.0],
            ampere_turns: at_per_layer,
        });
    }

    compute_field(&loops, point)
}

/// Generate field visualization data for Helmholtz coil configuration
pub fn compute_helmholtz_field(
    _coil_radius: f64,     // Mean radius (mm) - unused, kept for API compatibility
    coil_inner_r: f64,     // Inner radius (mm)
    coil_outer_r: f64,     // Outer radius (mm)
    coil_width: f64,       // Axial width (mm)
    gap: f64,              // Gap between coils (mm)
    ampere_turns: f64,     // A·turns per coil
    num_layers: usize,     // Radial layers to model
    plane: PlaneType,      // Plane orientation for 2D slice
    plane_offset: f64,     // Offset along plane normal (mm)
    colormap: Colormap,    // Colormap for visualization
) -> FieldData {
    // Create current loops to model the coils
    // Distribute loops across the cross-section
    let mut loops = Vec::new();

    let coil_z = gap / 2.0 + coil_width / 2.0;
    let dr = (coil_outer_r - coil_inner_r) / num_layers as f64;
    let at_per_layer = ampere_turns / num_layers as f64;

    for layer in 0..num_layers {
        let r = coil_inner_r + (layer as f64 + 0.5) * dr;

        // Upper coil
        loops.push(CurrentLoop {
            center: [0.0, 0.0, coil_z],
            radius: r,
            normal: [0.0, 0.0, 1.0],
            ampere_turns: at_per_layer,
        });

        // Lower coil
        loops.push(CurrentLoop {
            center: [0.0, 0.0, -coil_z],
            radius: r,
            normal: [0.0, 0.0, 1.0],
            ampere_turns: at_per_layer,
        });
    }

    // Compute 2D slice
    let slice_width = 80;
    let slice_height = 80;
    let extent = coil_outer_r * 2.5;
    let z_extent = (coil_z + coil_width) * 1.5;

    // Bounds depend on plane type
    let (axis1_min, axis1_max, axis2_min, axis2_max) = match plane {
        PlaneType::XZ => (-extent, extent, -z_extent, z_extent),
        PlaneType::XY => (-extent, extent, -extent, extent),
        PlaneType::YZ => (-extent, extent, -z_extent, z_extent),
    };

    let mut slice_bx = Vec::with_capacity(slice_width * slice_height);
    let mut slice_bz = Vec::with_capacity(slice_width * slice_height);
    let mut slice_magnitude = Vec::with_capacity(slice_width * slice_height);

    for j in 0..slice_height {
        let axis2 = axis2_min + (j as f64 + 0.5) * (axis2_max - axis2_min) / slice_height as f64;
        for i in 0..slice_width {
            let axis1 = axis1_min + (i as f64 + 0.5) * (axis1_max - axis1_min) / slice_width as f64;

            // Map axes to 3D point based on plane type
            let point = match plane {
                PlaneType::XZ => [axis1, plane_offset, axis2],  // x, y=offset, z
                PlaneType::XY => [axis1, axis2, plane_offset],  // x, y, z=offset
                PlaneType::YZ => [plane_offset, axis1, axis2],  // x=offset, y, z
            };

            let b = compute_field(&loops, point);
            let mag = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();

            // Store in-plane components based on plane type
            let (b1, b2) = match plane {
                PlaneType::XZ => (b[0], b[2]),  // Bx, Bz
                PlaneType::XY => (b[0], b[1]),  // Bx, By
                PlaneType::YZ => (b[1], b[2]),  // By, Bz
            };

            slice_bx.push(b1 as f32);
            slice_bz.push(b2 as f32);
            slice_magnitude.push(mag as f32);
        }
    }
    let slice_bounds = [axis1_min, axis1_max, axis2_min, axis2_max];

    // Compute 3D arrow field
    let arrow_grid = 10;
    let mut arrows_positions = Vec::new();
    let mut arrows_vectors = Vec::new();
    let mut arrows_magnitudes = Vec::new();

    let arrow_extent = coil_outer_r * 1.8;
    let arrow_z_extent = z_extent * 0.8;

    for k in 0..arrow_grid {
        let z = -arrow_z_extent + (k as f64 + 0.5) * 2.0 * arrow_z_extent / arrow_grid as f64;
        for j in 0..arrow_grid {
            let y = -arrow_extent + (j as f64 + 0.5) * 2.0 * arrow_extent / arrow_grid as f64;
            for i in 0..arrow_grid {
                let x = -arrow_extent + (i as f64 + 0.5) * 2.0 * arrow_extent / arrow_grid as f64;

                // Skip points inside the coils
                let rho = (x * x + y * y).sqrt();
                if rho > coil_inner_r * 0.9 && rho < coil_outer_r * 1.1 {
                    let z_abs = z.abs();
                    if z_abs > coil_z - coil_width && z_abs < coil_z + coil_width {
                        continue;
                    }
                }

                let b = compute_field(&loops, [x, y, z]);
                let mag = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();

                if mag > 1e-10 {
                    arrows_positions.push(x as f32);
                    arrows_positions.push(y as f32);
                    arrows_positions.push(z as f32);

                    // Normalize for display
                    arrows_vectors.push((b[0] / mag) as f32);
                    arrows_vectors.push((b[1] / mag) as f32);
                    arrows_vectors.push((b[2] / mag) as f32);

                    arrows_magnitudes.push(mag as f32);
                }
            }
        }
    }

    // Compute 1D line along Z axis
    let line_points = 101;
    let mut line_z = Vec::with_capacity(line_points);
    let mut line_bz = Vec::with_capacity(line_points);

    for i in 0..line_points {
        let z = -z_extent + i as f64 * (2.0 * z_extent) / (line_points - 1) as f64;
        let b = compute_field(&loops, [0.0, 0.0, z]);

        line_z.push(z as f32);
        line_bz.push(b[2] as f32);
    }

    FieldData {
        plane_type: plane,
        colormap,
        slice_width,
        slice_height,
        slice_bounds,
        slice_offset: plane_offset,
        slice_bx,
        slice_bz,
        slice_magnitude,
        arrows_positions,
        arrows_vectors,
        arrows_magnitudes,
        line_z,
        line_bz,
    }
}

// ===========================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_single_loop(radius_mm: f64, ampere_turns: f64) -> CurrentLoop {
        CurrentLoop {
            center: [0.0, 0.0, 0.0],
            radius: radius_mm,
            normal: [0.0, 0.0, 1.0],
            ampere_turns,
        }
    }

    #[test]
    fn test_single_loop_field_at_center() {
        // On-axis field at center of a single loop: B_z = mu_0 * I / (2R)
        let radius_mm = 50.0;
        let radius_m = radius_mm * 1e-3;
        let current = 1.0;

        let loop_ = make_single_loop(radius_mm, current);
        let b = biot_savart_loop(&loop_, [0.0, 0.0, 0.0], 256);

        let expected_bz = MU0 * current / (2.0 * radius_m);

        assert!(
            b[0].abs() < 1e-12,
            "Bx should be zero at center, got {}",
            b[0]
        );
        assert!(
            b[1].abs() < 1e-12,
            "By should be zero at center, got {}",
            b[1]
        );
        let rel_error = (b[2] - expected_bz).abs() / expected_bz;
        assert!(
            rel_error < 0.01,
            "Bz error too large: got {:.6e}, expected {:.6e}, rel_error = {:.4}",
            b[2],
            expected_bz,
            rel_error
        );
    }

    #[test]
    fn test_single_loop_field_on_axis() {
        // On-axis field at distance z: B_z = mu_0 * I * R^2 / (2 * (R^2 + z^2)^(3/2))
        let radius_mm = 50.0;
        let radius_m = radius_mm * 1e-3;
        let z_mm = 25.0;
        let z_m = z_mm * 1e-3;
        let current = 1.0;

        let loop_ = make_single_loop(radius_mm, current);
        let b = biot_savart_loop(&loop_, [0.0, 0.0, z_mm], 256);

        let r2_plus_z2 = radius_m * radius_m + z_m * z_m;
        let expected_bz = MU0 * current * radius_m * radius_m / (2.0 * r2_plus_z2.powf(1.5));

        assert!(
            b[0].abs() < 1e-12,
            "Bx should be zero on axis, got {}",
            b[0]
        );
        assert!(
            b[1].abs() < 1e-12,
            "By should be zero on axis, got {}",
            b[1]
        );
        let rel_error = (b[2] - expected_bz).abs() / expected_bz;
        assert!(
            rel_error < 0.01,
            "Bz error too large: got {:.6e}, expected {:.6e}, rel_error = {:.4}",
            b[2],
            expected_bz,
            rel_error
        );
    }

    #[test]
    fn test_helmholtz_field_at_center() {
        // Helmholtz coil: B_center = 0.7155 * mu_0 * n * I / R
        // Standard Helmholtz: gap between coils = radius (so coil centers at z = ±R/2)
        let radius_mm = 50.0;
        let radius_m = radius_mm * 1e-3;
        let current = 1.0;

        // Two loops at z = ±R/2 (Helmholtz spacing)
        let half_gap_mm = radius_mm / 2.0;
        let loops = vec![
            CurrentLoop {
                center: [0.0, 0.0, half_gap_mm],
                radius: radius_mm,
                normal: [0.0, 0.0, 1.0],
                ampere_turns: current,
            },
            CurrentLoop {
                center: [0.0, 0.0, -half_gap_mm],
                radius: radius_mm,
                normal: [0.0, 0.0, 1.0],
                ampere_turns: current,
            },
        ];

        let b = compute_field(&loops, [0.0, 0.0, 0.0]);
        let expected_bz = 0.7155 * MU0 * current / radius_m;

        assert!(
            b[0].abs() < 1e-12,
            "Bx should be zero at center, got {}",
            b[0]
        );
        assert!(
            b[1].abs() < 1e-12,
            "By should be zero at center, got {}",
            b[1]
        );
        let rel_error = (b[2] - expected_bz).abs() / expected_bz;
        assert!(
            rel_error < 0.02,
            "Helmholtz Bz error: got {:.6e}, expected {:.6e}, rel_error = {:.4}",
            b[2],
            expected_bz,
            rel_error
        );
    }

    #[test]
    fn test_helmholtz_field_uniformity() {
        // At Helmholtz condition (gap = R), field should be uniform near center
        // Check that dB/dz ≈ 0 by comparing field at center vs small offset
        let radius_mm = 50.0;
        let current = 1.0;

        let half_gap_mm = radius_mm / 2.0;
        let loops = vec![
            CurrentLoop {
                center: [0.0, 0.0, half_gap_mm],
                radius: radius_mm,
                normal: [0.0, 0.0, 1.0],
                ampere_turns: current,
            },
            CurrentLoop {
                center: [0.0, 0.0, -half_gap_mm],
                radius: radius_mm,
                normal: [0.0, 0.0, 1.0],
                ampere_turns: current,
            },
        ];

        let b_center = compute_field(&loops, [0.0, 0.0, 0.0]);
        let offset = 5.0; // 5mm offset (10% of radius)
        let b_plus = compute_field(&loops, [0.0, 0.0, offset]);
        let b_minus = compute_field(&loops, [0.0, 0.0, -offset]);

        // Field should be symmetric
        let symmetry_error = (b_plus[2] - b_minus[2]).abs() / b_center[2];
        assert!(
            symmetry_error < 1e-10,
            "Field not symmetric: Bz(+z) = {:.6e}, Bz(-z) = {:.6e}",
            b_plus[2],
            b_minus[2]
        );

        // Field should be nearly constant (Helmholtz condition: d²B/dz² = 0 at center)
        let uniformity_error = (b_plus[2] - b_center[2]).abs() / b_center[2];
        assert!(
            uniformity_error < 0.01,
            "Field not uniform: Bz(0) = {:.6e}, Bz(±5mm) = {:.6e}, variation = {:.4}%",
            b_center[2],
            b_plus[2],
            uniformity_error * 100.0
        );
    }

    #[test]
    fn test_compute_helmholtz_field_structure() {
        let field_data = compute_helmholtz_field(
            50.0,  // coil_radius (unused)
            45.0,  // coil_inner_r
            55.0,  // coil_outer_r
            10.0,  // coil_width
            50.0,  // gap (Helmholtz: gap = mean radius)
            1.0,   // ampere_turns
            4,     // num_layers
            PlaneType::XZ,
            0.0,   // plane_offset
            Colormap::Jet,
        );

        assert_eq!(field_data.slice_width, 80);
        assert_eq!(field_data.slice_height, 80);
        assert_eq!(field_data.slice_bx.len(), 80 * 80);
        assert_eq!(field_data.slice_bz.len(), 80 * 80);
        assert_eq!(field_data.slice_magnitude.len(), 80 * 80);
        assert_eq!(field_data.line_z.len(), 101);
        assert_eq!(field_data.line_bz.len(), 101);
        assert!(field_data.arrows_positions.len() > 0);
        assert_eq!(field_data.arrows_positions.len() % 3, 0);
        assert_eq!(
            field_data.arrows_positions.len(),
            field_data.arrows_vectors.len()
        );
        assert_eq!(
            field_data.arrows_magnitudes.len() * 3,
            field_data.arrows_positions.len()
        );
    }

    #[test]
    fn test_field_data_to_binary() {
        let field_data = compute_helmholtz_field(
            50.0, 45.0, 55.0, 10.0, 50.0, 1.0, 2, PlaneType::XZ, 0.0, Colormap::Jet,
        );

        let binary = field_data.to_binary();

        // Check header
        assert_eq!(&binary[0..6], b"FIELD\0");

        // Check dimensions (u32 little-endian)
        let width = u32::from_le_bytes([binary[8], binary[9], binary[10], binary[11]]);
        let height = u32::from_le_bytes([binary[12], binary[13], binary[14], binary[15]]);
        assert_eq!(width, 80);
        assert_eq!(height, 80);
    }

    #[test]
    fn test_colormap_from_str() {
        assert_eq!(Colormap::from_str("jet"), Colormap::Jet);
        assert_eq!(Colormap::from_str("viridis"), Colormap::Viridis);
        assert_eq!(Colormap::from_str("VIRIDIS"), Colormap::Viridis);
        assert_eq!(Colormap::from_str("plasma"), Colormap::Plasma);
        assert_eq!(Colormap::from_str("unknown"), Colormap::Jet);
    }

    #[test]
    fn test_x_aligned_loop_field_at_center() {
        let radius_mm = 50.0;
        let radius_m = radius_mm * 1e-3;
        let current = 1.0;

        let loop_ = CurrentLoop {
            center: [0.0, 0.0, 0.0],
            radius: radius_mm,
            normal: [1.0, 0.0, 0.0],
            ampere_turns: current,
        };
        let b = biot_savart_loop(&loop_, [0.0, 0.0, 0.0], 256);

        let expected_bx = MU0 * current / (2.0 * radius_m);

        let rel_error = (b[0] - expected_bx).abs() / expected_bx;
        assert!(
            rel_error < 0.01,
            "Bx error too large: got {:.6e}, expected {:.6e}, rel_error = {:.4}",
            b[0],
            expected_bx,
            rel_error
        );
        assert!(
            b[1].abs() < 1e-12,
            "By should be zero at center, got {}",
            b[1]
        );
        assert!(
            b[2].abs() < 1e-12,
            "Bz should be zero at center, got {}",
            b[2]
        );
    }
}
