use anyhow::{anyhow, Result};
use mlua::{Table, Value};
use serde::Serialize;
use serde_json::{Map, Value as JsonValue};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct SceneIr {
    pub kind: String,
    pub objects: Vec<ObjectIr>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectIr {
    #[serde(rename = "type")]
    pub obj_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<JsonValue>,
    pub transform: TransformIr,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ObjectIr>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TransformIr {
    pub matrix: [[f64; 4]; 4],
}

#[derive(Debug, Clone, Copy)]
struct TransformOp {
    op: TransformOpType,
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransformOpType {
    Translate,
    Rotate,
    Scale,
}

pub fn scene_from_lua_value(value: &Value) -> Result<SceneIr> {
    let scene_table = value
        .as_table()
        .ok_or_else(|| anyhow!("Lua script must return scene table"))?;
    let objects = scene_table
        .get::<_, Table>("objects")
        .map_err(|_| anyhow!("Scene table missing 'objects'"))?;
    scene_from_objects_table(&objects)
}

pub fn scene_from_objects_table(objects: &Table) -> Result<SceneIr> {
    let mut indexed_objects: Vec<(i64, ObjectIr)> = Vec::new();
    for pair in objects.clone().pairs::<i64, Table>() {
        let (idx, object_table) = pair?;
        indexed_objects.push((idx, canonicalize_object(&object_table)?));
    }
    indexed_objects.sort_by_key(|(idx, _)| *idx);

    let mut objects: Vec<(String, ObjectIr)> = indexed_objects
        .into_iter()
        .map(|(_, object)| {
            let object_text = serde_json::to_string(&object).unwrap_or_default();
            (stable_hash_hex(&object_text), object)
        })
        .collect();

    objects.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(SceneIr {
        kind: "scene".to_string(),
        objects: objects.into_iter().map(|(_, value)| value).collect(),
    })
}

pub fn scene_hash(scene: &SceneIr) -> Result<String> {
    let text = serde_json::to_string(scene)?;
    Ok(stable_hash_hex(&text))
}

pub fn stable_hash_hex(input: &str) -> String {
    // Deterministic FNV-1a 64-bit hash for content-addressable scene IDs.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in input.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

fn canonicalize_object(table: &Table) -> Result<ObjectIr> {
    let obj_type: String = table.get("type")?;

    let name = table
        .get::<_, String>("name")
        .ok()
        .filter(|s| !s.is_empty());

    let operation = table
        .get::<_, String>("operation")
        .ok()
        .filter(|s| !s.is_empty());

    let component = table
        .get::<_, String>("component")
        .ok()
        .filter(|s| !s.is_empty());

    let params = table
        .get::<_, Table>("params")
        .ok()
        .map(canonicalize_table)
        .transpose()?;

    let material = table
        .get::<_, Value>("material")
        .ok()
        .map(canonicalize_lua_value)
        .transpose()?
        .filter(|v| !v.is_null());

    let color = table
        .get::<_, Value>("color")
        .ok()
        .map(canonicalize_lua_value)
        .transpose()?
        .filter(|v| !v.is_null());

    let transform = table
        .get::<_, Table>("ops")
        .ok()
        .map(ops_table_to_transform)
        .transpose()?
        .unwrap_or_else(TransformIr::identity);

    let mut children = if let Ok(children_table) = table.get::<_, Table>("children") {
        let mut indexed_children: Vec<(i64, ObjectIr)> = Vec::new();
        for pair in children_table.clone().pairs::<i64, Table>() {
            let (idx, child_table) = pair?;
            indexed_children.push((idx, canonicalize_object(&child_table)?));
        }
        indexed_children.sort_by_key(|(idx, _)| *idx);
        indexed_children
            .into_iter()
            .map(|(_, child)| child)
            .collect()
    } else {
        Vec::new()
    };

    if obj_type == "csg" {
        if let Some(op) = &operation {
            if op == "union" || op == "intersect" {
                children.sort_by(|a, b| {
                    let a_text = serde_json::to_string(a).unwrap_or_default();
                    let b_text = serde_json::to_string(b).unwrap_or_default();
                    a_text.cmp(&b_text)
                });
            }
        }
    }

    Ok(ObjectIr {
        obj_type,
        name,
        operation,
        component,
        params,
        transform,
        material,
        color,
        children,
    })
}

fn ops_table_to_transform(ops: Table) -> Result<TransformIr> {
    let mut indexed_ops: Vec<(i64, TransformOp)> = Vec::new();

    for pair in ops.clone().pairs::<i64, Table>() {
        let (idx, op_table) = pair?;
        let op_name: String = op_table.get("op").unwrap_or_default();
        let x: f64 = op_table.get("x").unwrap_or(0.0);
        let y: f64 = op_table.get("y").unwrap_or(0.0);
        let z: f64 = op_table.get("z").unwrap_or(0.0);

        let op = match op_name.as_str() {
            "translate" => TransformOpType::Translate,
            "rotate" => TransformOpType::Rotate,
            "scale" => TransformOpType::Scale,
            _ => continue,
        };

        indexed_ops.push((idx, TransformOp { op, x, y, z }));
    }

    indexed_ops.sort_by_key(|(idx, _)| *idx);
    let ops: Vec<TransformOp> = indexed_ops.into_iter().map(|(_, op)| op).collect();
    Ok(TransformIr::from_ops(&ops))
}

impl TransformIr {
    pub fn identity() -> Self {
        Self {
            matrix: identity_matrix(),
        }
    }

    fn from_ops(ops: &[TransformOp]) -> Self {
        let mut matrix = identity_matrix();

        for op in ops {
            let op_matrix = match op.op {
                TransformOpType::Translate => translation_matrix(op.x, op.y, op.z),
                TransformOpType::Rotate => rotation_zyx_matrix(op.x, op.y, op.z),
                TransformOpType::Scale => scale_matrix(op.x, op.y, op.z),
            };
            // Preserve current runtime semantics: apply operations in declaration order.
            matrix = mat_mul(op_matrix, matrix);
        }

        Self {
            matrix: round_matrix(matrix),
        }
    }
}

fn canonicalize_lua_value(value: Value) -> Result<JsonValue> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(b) => Ok(JsonValue::Bool(b)),
        Value::Integer(i) => Ok(JsonValue::from(i)),
        Value::Number(n) => Ok(JsonValue::from(round_f64(n))),
        Value::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        Value::Table(t) => canonicalize_table(t),
        Value::LightUserData(_) => Ok(JsonValue::String("<light_userdata>".to_string())),
        Value::Function(_) => Ok(JsonValue::String("<function>".to_string())),
        Value::Thread(_) => Ok(JsonValue::String("<thread>".to_string())),
        Value::UserData(_) => Ok(JsonValue::String("<userdata>".to_string())),
        Value::Error(e) => Ok(JsonValue::String(format!("<error:{}>", e))),
    }
}

fn canonicalize_table(table: Table) -> Result<JsonValue> {
    let mut array_entries: Vec<(i64, JsonValue)> = Vec::new();
    let mut map_entries: BTreeMap<String, JsonValue> = BTreeMap::new();
    let mut saw_non_array_key = false;

    for pair in table.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let canonical_value = canonicalize_lua_value(value)?;

        match key {
            Value::Integer(i) if i > 0 => {
                array_entries.push((i, canonical_value));
            }
            Value::String(s) => {
                saw_non_array_key = true;
                map_entries.insert(s.to_str()?.to_string(), canonical_value);
            }
            Value::Number(n) if n.fract() == 0.0 && n > 0.0 => {
                array_entries.push((n as i64, canonical_value));
            }
            Value::Boolean(b) => {
                saw_non_array_key = true;
                map_entries.insert(b.to_string(), canonical_value);
            }
            Value::Integer(i) => {
                saw_non_array_key = true;
                map_entries.insert(i.to_string(), canonical_value);
            }
            Value::Number(n) => {
                saw_non_array_key = true;
                map_entries.insert(round_f64(n).to_string(), canonical_value);
            }
            _ => {
                saw_non_array_key = true;
                map_entries.insert("<unsupported_key>".to_string(), canonical_value);
            }
        }
    }

    array_entries.sort_by_key(|(idx, _)| *idx);
    let contiguous_array = !array_entries.is_empty()
        && array_entries
            .iter()
            .enumerate()
            .all(|(i, (idx, _))| *idx == (i as i64 + 1));

    if contiguous_array && !saw_non_array_key {
        let values: Vec<JsonValue> = array_entries.into_iter().map(|(_, value)| value).collect();
        return Ok(JsonValue::Array(values));
    }

    for (idx, value) in array_entries {
        map_entries.insert(idx.to_string(), value);
    }

    Ok(ordered_object_json(map_entries))
}

fn ordered_object_json(entries: BTreeMap<String, JsonValue>) -> JsonValue {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key, value);
    }
    JsonValue::Object(map)
}

fn round_f64(value: f64) -> f64 {
    let scaled = (value * 1_000_000_000.0).round() / 1_000_000_000.0;
    if scaled == -0.0 {
        0.0
    } else {
        scaled
    }
}

fn round_matrix(matrix: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut out = matrix;
    for row in &mut out {
        for value in row {
            *value = round_f64(*value);
        }
    }
    out
}

fn identity_matrix() -> [[f64; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn translation_matrix(x: f64, y: f64, z: f64) -> [[f64; 4]; 4] {
    [
        [1.0, 0.0, 0.0, x],
        [0.0, 1.0, 0.0, y],
        [0.0, 0.0, 1.0, z],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn scale_matrix(x: f64, y: f64, z: f64) -> [[f64; 4]; 4] {
    [
        [x, 0.0, 0.0, 0.0],
        [0.0, y, 0.0, 0.0],
        [0.0, 0.0, z, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn rotation_zyx_matrix(x_deg: f64, y_deg: f64, z_deg: f64) -> [[f64; 4]; 4] {
    let rx = x_deg.to_radians();
    let ry = y_deg.to_radians();
    let rz = z_deg.to_radians();

    let (sx, cx) = (rx.sin(), rx.cos());
    let (sy, cy) = (ry.sin(), ry.cos());
    let (sz, cz) = (rz.sin(), rz.cos());

    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz, 0.0],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz, 0.0],
        [-sy, sx * cy, cx * cy, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn mat_mul(a: [[f64; 4]; 4], b: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut out = [[0.0f64; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            out[r][c] =
                a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c] + a[r][3] * b[3][c];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_ops_point(mut p: [f64; 3], ops: &[TransformOp]) -> [f64; 3] {
        for op in ops {
            match op.op {
                TransformOpType::Translate => {
                    p[0] += op.x;
                    p[1] += op.y;
                    p[2] += op.z;
                }
                TransformOpType::Rotate => {
                    let m = rotation_zyx_matrix(op.x, op.y, op.z);
                    let x = m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2];
                    let y = m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2];
                    let z = m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2];
                    p = [x, y, z];
                }
                TransformOpType::Scale => {
                    p[0] *= op.x;
                    p[1] *= op.y;
                    p[2] *= op.z;
                }
            }
        }
        p
    }

    fn apply_matrix_point(matrix: [[f64; 4]; 4], p: [f64; 3]) -> [f64; 3] {
        [
            matrix[0][0] * p[0] + matrix[0][1] * p[1] + matrix[0][2] * p[2] + matrix[0][3],
            matrix[1][0] * p[0] + matrix[1][1] * p[1] + matrix[1][2] * p[2] + matrix[1][3],
            matrix[2][0] * p[0] + matrix[2][1] * p[1] + matrix[2][2] * p[2] + matrix[2][3],
        ]
    }

    fn assert_point_close(a: [f64; 3], b: [f64; 3], eps: f64) {
        assert!((a[0] - b[0]).abs() <= eps, "x mismatch: {:?} vs {:?}", a, b);
        assert!((a[1] - b[1]).abs() <= eps, "y mismatch: {:?} vs {:?}", a, b);
        assert!((a[2] - b[2]).abs() <= eps, "z mismatch: {:?} vs {:?}", a, b);
    }

    #[test]
    fn merged_matrix_preserves_transform_order() {
        let ops = vec![
            TransformOp {
                op: TransformOpType::Translate,
                x: 12.0,
                y: -3.0,
                z: 7.5,
            },
            TransformOp {
                op: TransformOpType::Rotate,
                x: 0.0,
                y: 0.0,
                z: 90.0,
            },
            TransformOp {
                op: TransformOpType::Scale,
                x: 1.2,
                y: 0.8,
                z: 2.0,
            },
            TransformOp {
                op: TransformOpType::Rotate,
                x: -15.0,
                y: 20.0,
                z: 5.0,
            },
        ];

        let merged = TransformIr::from_ops(&ops);

        for p in [
            [0.0, 0.0, 0.0],
            [1.0, 2.0, 3.0],
            [-4.0, 8.0, 0.5],
            [10.0, -2.0, 11.0],
        ] {
            let old = apply_ops_point(p, &ops);
            let new = apply_matrix_point(merged.matrix, p);
            assert_point_close(old, new, 1e-8);
        }
    }

    #[test]
    fn scene_ir_contains_merged_transform() {
        let lua = mlua::Lua::new();
        let scene: Value = lua
            .load(
                r#"
                local scene = {
                  objects = {
                    {
                      type = "cylinder",
                      params = { r = 10, h = 20 },
                      ops = {
                        { op = "rotate", x = 0, y = 0, z = 90 },
                        { op = "translate", x = 5, y = 6, z = 7 },
                      }
                    }
                  }
                }
                return scene
                "#,
            )
            .eval()
            .expect("lua eval");

        let ir = scene_from_lua_value(&scene).expect("scene ir");
        assert_eq!(ir.kind, "scene");
        assert_eq!(ir.objects.len(), 1);
        let obj = &ir.objects[0];
        assert_eq!(obj.obj_type, "cylinder");
        assert!(obj.params.is_some());
        assert_ne!(obj.transform.matrix, identity_matrix());
    }
}
