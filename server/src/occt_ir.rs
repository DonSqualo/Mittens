//! OCCT-based export of canonical IR scenes to STEP.
//!
//! Notes:
//! - This is intentionally conservative: OCCT `gp_Trsf` does not support arbitrary
//!   affine transforms in our wrapper, so we decompose the IR 4x4 matrix into
//!   uniform scale + rotation + translation and apply them sequentially.
//! - Non-uniform scale will be rejected for STEP export.

use crate::ir;
use anyhow::{anyhow, bail, Context, Result};

#[cfg(feature = "occt-support")]
use cxx::UniquePtr;

#[cfg(feature = "occt-support")]
use opencascade_sys::ffi;

#[cfg(feature = "occt-support")]
use std::collections::HashMap;

#[cfg(feature = "occt-support")]
use std::path::{Path, PathBuf};

#[cfg(feature = "occt-support")]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "occt-support")]
struct OcctShape {
    inner: UniquePtr<ffi::TopoDS_Shape>,
}

#[cfg(feature = "occt-support")]
impl OcctShape {
    fn from_ref(shape: &ffi::TopoDS_Shape) -> Self {
        Self {
            inner: ffi::TopoDS_Shape_to_owned(shape),
        }
    }

    fn as_ref(&self) -> Result<&ffi::TopoDS_Shape> {
        self.inner
            .as_ref()
            .ok_or_else(|| anyhow!("OCCT shape is null"))
    }

    fn union(&self, other: &Self) -> Result<Self> {
        let mut op = ffi::BRepAlgoAPI_Fuse_ctor(self.as_ref()?, other.as_ref()?);
        Ok(Self::from_ref(op.pin_mut().Shape()))
    }

    fn subtract(&self, other: &Self) -> Result<Self> {
        let mut op = ffi::BRepAlgoAPI_Cut_ctor(self.as_ref()?, other.as_ref()?);
        Ok(Self::from_ref(op.pin_mut().Shape()))
    }

    fn intersect(&self, other: &Self) -> Result<Self> {
        let mut op = ffi::BRepAlgoAPI_Common_ctor(self.as_ref()?, other.as_ref()?);
        Ok(Self::from_ref(op.pin_mut().Shape()))
    }

    fn transform(&self, trsf: &ffi::gp_Trsf) -> Result<Self> {
        let mut op = ffi::BRepBuilderAPI_Transform_ctor(self.as_ref()?, trsf, false);
        let progress = ffi::Message_ProgressRange_ctor();
        op.pin_mut().Build(&progress);
        if !op.IsDone() {
            bail!("OCCT transform failed");
        }
        Ok(Self::from_ref(op.pin_mut().Shape()))
    }

    fn clean(self) -> Self {
        let mut upgrader = ffi::ShapeUpgrade_UnifySameDomain_ctor(self.inner.as_ref().unwrap(), true, true, true);
        upgrader.pin_mut().AllowInternalEdges(false);
        upgrader.pin_mut().Build();
        Self::from_ref(upgrader.Shape())
    }
}

#[cfg(feature = "occt-support")]
fn build_single_export_packet(filename: &str, payload: &[u8]) -> Vec<u8> {
    let name_bytes = filename.as_bytes();
    let mut packet = Vec::with_capacity(8 + 1 + 4 + name_bytes.len() + 4 + payload.len());
    packet.extend_from_slice(b"EXPORT\0\0");
    packet.push(1u8); // single file
    packet.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
    packet.extend_from_slice(name_bytes);
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(payload);
    packet
}

#[cfg(feature = "occt-support")]
fn apply_transform_matrix(mut shape: OcctShape, m: [[f64; 4]; 4]) -> Result<OcctShape> {
    // IR uses row-major matrices with translation in last column.
    let t = [m[0][3], m[1][3], m[2][3]];

    // Upper-left 3x3.
    let a00 = m[0][0];
    let a01 = m[0][1];
    let a02 = m[0][2];
    let a10 = m[1][0];
    let a11 = m[1][1];
    let a12 = m[1][2];
    let a20 = m[2][0];
    let a21 = m[2][1];
    let a22 = m[2][2];

    // Column lengths => scale along axes (assuming no shear).
    let sx = (a00 * a00 + a10 * a10 + a20 * a20).sqrt();
    let sy = (a01 * a01 + a11 * a11 + a21 * a21).sqrt();
    let sz = (a02 * a02 + a12 * a12 + a22 * a22).sqrt();

    let eps = 1e-7;
    if sx < eps || sy < eps || sz < eps {
        bail!("degenerate transform matrix (near-zero scale)");
    }

    // Enforce uniform scale for STEP export.
    let s = (sx + sy + sz) / 3.0;
    if (sx - s).abs() > 1e-6 || (sy - s).abs() > 1e-6 || (sz - s).abs() > 1e-6 {
        bail!(
            "non-uniform scale not supported for STEP export (sx={}, sy={}, sz={})",
            sx,
            sy,
            sz
        );
    }

    // Normalize to pure rotation.
    let r00 = a00 / s;
    let r01 = a01 / s;
    let r02 = a02 / s;
    let r10 = a10 / s;
    let r11 = a11 / s;
    let r12 = a12 / s;
    let r20 = a20 / s;
    let r21 = a21 / s;
    let r22 = a22 / s;

    // Rotation -> axis/angle.
    let trace = r00 + r11 + r22;
    let mut cos_theta = (trace - 1.0) * 0.5;
    if cos_theta > 1.0 {
        cos_theta = 1.0;
    }
    if cos_theta < -1.0 {
        cos_theta = -1.0;
    }
    let angle = cos_theta.acos();

    let origin = ffi::new_point(0.0, 0.0, 0.0);

    // Apply uniform scale about origin, if needed.
    if (s - 1.0).abs() > 1e-9 {
        let mut trsf = ffi::new_transform();
        trsf.pin_mut().SetScale(&origin, s);
        shape = shape.transform(&trsf)?;
    }

    // Apply rotation about origin, if needed.
    if angle.abs() > 1e-9 {
        // Handle near-pi rotations carefully.
        let sin_theta = angle.sin();
        let (ax, ay, az) = if sin_theta.abs() > 1e-7 {
            (
                (r21 - r12) / (2.0 * sin_theta),
                (r02 - r20) / (2.0 * sin_theta),
                (r10 - r01) / (2.0 * sin_theta),
            )
        } else {
            // Fallback: pick axis from diagonal.
            let x = ((r00 + 1.0) * 0.5).max(0.0).sqrt();
            let y = ((r11 + 1.0) * 0.5).max(0.0).sqrt();
            let z = ((r22 + 1.0) * 0.5).max(0.0).sqrt();
            (x, y, z)
        };

        let norm = (ax * ax + ay * ay + az * az).sqrt();
        if norm > 1e-12 {
            let dir = ffi::gp_Dir_ctor(ax / norm, ay / norm, az / norm);
            let axis = ffi::gp_Ax1_ctor(&origin, &dir);
            let mut trsf = ffi::new_transform();
            trsf.pin_mut().SetRotation(&axis, angle);
            shape = shape.transform(&trsf)?;
        }
    }

    // Apply translation, if needed.
    if t[0].abs() > 1e-12 || t[1].abs() > 1e-12 || t[2].abs() > 1e-12 {
        let mut trsf = ffi::new_transform();
        let v = ffi::new_vec(t[0], t[1], t[2]);
        trsf.pin_mut().set_translation_vec(&v);
        shape = shape.transform(&trsf)?;
    }

    Ok(shape)
}

#[cfg(feature = "occt-support")]
fn json_f64(params: &serde_json::Value, key: &str) -> Result<f64> {
    params
        .get(key)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| anyhow!("missing/invalid param '{}' (expected number)", key))
}

#[cfg(feature = "occt-support")]
fn json_f64_opt(params: &serde_json::Value, key: &str) -> Option<f64> {
    params.get(key).and_then(|v| v.as_f64())
}

#[cfg(feature = "occt-support")]
fn build_primitive(obj: &ir::ObjectIr) -> Result<OcctShape> {
    let params = obj
        .params
        .as_ref()
        .ok_or_else(|| anyhow!("primitive '{}' missing params", obj.obj_type))?;

    match obj.obj_type.as_str() {
        "box" => {
            let w = json_f64(params, "w")?;
            let d = json_f64_opt(params, "d").unwrap_or(w);
            let h = json_f64(params, "h")?;
            let p = ffi::new_point(0.0, 0.0, 0.0);
            let mut maker = ffi::BRepPrimAPI_MakeBox_ctor(&p, w, d, h);
            Ok(OcctShape::from_ref(maker.pin_mut().Shape()))
        }
        "cylinder" => {
            let r = json_f64(params, "r")?;
            let h = json_f64(params, "h")?;
            let p = ffi::new_point(0.0, 0.0, 0.0);
            let ax2 = ffi::gp_Ax2_ctor(&p, ffi::gp_DZ());
            let mut maker = ffi::BRepPrimAPI_MakeCylinder_ctor(&ax2, r, h);
            Ok(OcctShape::from_ref(maker.pin_mut().Shape()))
        }
        "sphere" => {
            let r = json_f64(params, "r")?;
            let mut maker = ffi::BRepPrimAPI_MakeSphere_ctor(r);
            Ok(OcctShape::from_ref(maker.pin_mut().Shape()))
        }
        "ring" => {
            // Match Manifold ring: outer cylinder minus slightly taller inner cylinder.
            let inner_radius = json_f64(params, "inner_radius")?;
            let outer_radius = json_f64(params, "outer_radius")?;
            let h = json_f64(params, "h")?;

            let p = ffi::new_point(0.0, 0.0, 0.0);
            let ax2 = ffi::gp_Ax2_ctor(&p, ffi::gp_DZ());

            let mut outer = ffi::BRepPrimAPI_MakeCylinder_ctor(&ax2, outer_radius, h);
            let mut inner = ffi::BRepPrimAPI_MakeCylinder_ctor(&ax2, inner_radius, h + 0.01);

            let outer_shape = OcctShape::from_ref(outer.pin_mut().Shape());
            let inner_shape = OcctShape::from_ref(inner.pin_mut().Shape());
            outer_shape.subtract(&inner_shape)
        }
        other => bail!("primitive '{}' not supported for STEP export", other),
    }
}

#[cfg(feature = "occt-support")]
fn collect_components<'a>(obj: &'a ir::ObjectIr, out: &mut HashMap<&'a str, &'a ir::ObjectIr>) {
    if obj.obj_type == "component" {
        if let Some(name) = obj.name.as_deref() {
            out.insert(name, obj);
        }
    }
    for child in &obj.children {
        collect_components(child, out);
    }
}

#[cfg(feature = "occt-support")]
fn build_shape_recursive(
    obj: &ir::ObjectIr,
    components: &HashMap<&str, &ir::ObjectIr>,
) -> Result<OcctShape> {
    match obj.obj_type.as_str() {
        "instance" => {
            let name = obj
                .component
                .as_deref()
                .ok_or_else(|| anyhow!("instance missing component name"))?;
            let def = components
                .get(name)
                .copied()
                .ok_or_else(|| anyhow!("component '{}' not found for instance", name))?;

            let base = build_shape_recursive(def, components)?;
            apply_transform_matrix(base, obj.transform.matrix)
        }
        "csg" => {
            let op = obj
                .operation
                .as_deref()
                .ok_or_else(|| anyhow!("csg missing operation"))?;
            if obj.children.is_empty() {
                bail!("csg has no children");
            }
            let mut acc = build_shape_recursive(&obj.children[0], components)?;
            for child in obj.children.iter().skip(1) {
                let rhs = build_shape_recursive(child, components)?;
                acc = match op {
                    "union" => acc.union(&rhs)?,
                    "difference" => acc.subtract(&rhs)?,
                    "intersect" => acc.intersect(&rhs)?,
                    other => bail!("unknown csg operation '{}'", other),
                };
            }
            apply_transform_matrix(acc, obj.transform.matrix)
        }
        "group" | "assembly" | "component" => {
            if obj.children.is_empty() {
                bail!("{} has no children", obj.obj_type);
            }
            let mut compound = ffi::TopoDS_Compound_ctor();
            let builder = ffi::BRep_Builder_ctor();
            let builder = ffi::BRep_Builder_upcast_to_topods_builder(&builder);
            builder.MakeCompound(compound.pin_mut());
            let mut compound_shape = ffi::TopoDS_Compound_as_shape(compound);

            for child in &obj.children {
                let child_shape = build_shape_recursive(child, components)?;
                builder.Add(
                    compound_shape.pin_mut(),
                    child_shape
                        .inner
                        .as_ref()
                        .ok_or_else(|| anyhow!("failed to build child shape"))?,
                );
            }

            let inner = ffi::TopoDS_cast_to_compound(&compound_shape);
            let inner = ffi::TopoDS_Compound_to_owned(inner);
            let shape = OcctShape::from_ref(ffi::cast_compound_to_shape(
                inner.as_ref().ok_or_else(|| anyhow!("failed to build compound"))?,
            ))
            .clean();
            apply_transform_matrix(shape, obj.transform.matrix)
        }
        _ => {
            let prim = build_primitive(obj)?;
            apply_transform_matrix(prim, obj.transform.matrix)
        }
    }
}

#[cfg(feature = "occt-support")]
pub fn export_step_packet_for_scene(scene: &ir::SceneIr, out_filename: &str) -> Result<Vec<u8>> {
    let mut components = HashMap::<&str, &ir::ObjectIr>::new();
    for obj in &scene.objects {
        collect_components(obj, &mut components);
    }

    let mut compound = ffi::TopoDS_Compound_ctor();
    let builder = ffi::BRep_Builder_ctor();
    let builder = ffi::BRep_Builder_upcast_to_topods_builder(&builder);
    builder.MakeCompound(compound.pin_mut());
    let mut compound_shape = ffi::TopoDS_Compound_as_shape(compound);

    for obj in &scene.objects {
        let s = build_shape_recursive(obj, &components)?;
        builder.Add(
            compound_shape.pin_mut(),
            s.inner
                .as_ref()
                .ok_or_else(|| anyhow!("failed to build top-level shape"))?,
        );
    }

    let inner = ffi::TopoDS_cast_to_compound(&compound_shape);
    let inner = ffi::TopoDS_Compound_to_owned(inner);
    let root = OcctShape::from_ref(ffi::cast_compound_to_shape(
        inner.as_ref().ok_or_else(|| anyhow!("failed to build compound"))?,
    ))
    .clean();

    // Write via OCCT's STEP writer (expects a path).
    let tmp_dir = std::env::var_os("MITTENS_STEP_EXPORT_TMP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tmp_path = tmp_dir.join(format!("mittens_export_{}_{}.step", std::process::id(), nonce));

    {
        let mut writer = ffi::STEPControl_Writer_ctor();
        let status = ffi::transfer_shape(writer.pin_mut(), root.as_ref()?);
        if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
            bail!("STEP transfer failed");
        }
        let status = ffi::write_step(writer.pin_mut(), tmp_path.to_string_lossy().to_string());
        if status != ffi::IFSelect_ReturnStatus::IFSelect_RetDone {
            bail!("STEP write failed");
        }
    }

    let bytes = std::fs::read(&tmp_path)
        .with_context(|| format!("failed to read STEP {}", tmp_path.display()))?;
    let _ = std::fs::remove_file(&tmp_path);

    Ok(build_single_export_packet(out_filename, &bytes))
}

#[cfg(not(feature = "occt-support"))]
pub fn export_step_packet_for_scene(_scene: &ir::SceneIr, _out_filename: &str) -> Result<Vec<u8>> {
    bail!("STEP export requires server feature 'occt-support'")
}

#[cfg(feature = "occt-support")]
pub fn export_step_packet_for_file(path: &Path) -> Result<Vec<u8>> {
    let data = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("invalid filename {}", path.display()))?;
    Ok(build_single_export_packet(name, &data))
}

#[cfg(not(feature = "occt-support"))]
pub fn export_step_packet_for_file(_path: &Path) -> Result<Vec<u8>> {
    bail!("STEP export requires server feature 'occt-support'")
}
