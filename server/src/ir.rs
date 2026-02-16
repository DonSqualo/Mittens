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
    let table = value
        .as_table()
        .ok_or_else(|| anyhow!("Lua script must return a scene table"))?;
    let objects = table
        .get::<_, Table>("objects")
        .map_err(|_| anyhow!("Scene table missing 'objects'"))?;
    scene_from_objects_table(&objects)
}

pub fn scene_from_objects_table(objects: &Table) -> Result<SceneIr> {
    let mut indexed: Vec<(i64, ObjectIr)> = Vec::new();
    for pair in objects.clone().pairs::<i64, Table>() {
        let (idx, obj) = pair?;
        indexed.push((idx, object_from_lua_table(&obj)?));
    }
    indexed.sort_by_key(|(idx, _)| *idx);

    let mut hashed: Vec<(String, ObjectIr)> = indexed
        .into_iter()
        .map(|(_, obj)| {
            let txt = serde_json::to_string(&obj).unwrap_or_default();
            (stable_hash_hex(&txt), obj)
        })
        .collect();
    hashed.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(SceneIr {
        kind: "scene".to_string(),
        objects: hashed.into_iter().map(|(_, obj)| obj).collect(),
    })
}

pub fn object_from_lua_table(table: &Table) -> Result<ObjectIr> {
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
        .map(canonicalize_value)
        .transpose()?
        .filter(|v| !v.is_null());

    let color = table
        .get::<_, Value>("color")
        .ok()
        .map(canonicalize_value)
        .transpose()?
        .filter(|v| !v.is_null());

    let transform = canonicalize_transform(table.get::<_, Table>("ops").ok())?;

    let mut children = if let Ok(child_table) = table.get::<_, Table>("children") {
        let mut indexed_children: Vec<(i64, ObjectIr)> = Vec::new();
        for pair in child_table.clone().pairs::<i64, Table>() {
            let (idx, child) = pair?;
            indexed_children.push((idx, object_from_lua_table(&child)?));
        }
        indexed_children.sort_by_key(|(idx, _)| *idx);
        indexed_children.into_iter().map(|(_, c)| c).collect()
    } else {
        Vec::new()
    };

    if obj_type == "csg" {
        if let Some(op) = &operation {
            if op == "union" || op == "intersect" {
                children.sort_by(|a, b| {
                    serde_json::to_string(a)
                        .unwrap_or_default()
                        .cmp(&serde_json::to_string(b).unwrap_or_default())
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

pub fn scene_hash(scene: &SceneIr) -> Result<String> {
    Ok(stable_hash_hex(&serde_json::to_string(scene)?))
}

pub fn stable_hash_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in input.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn canonicalize_transform(ops: Option<Table>) -> Result<TransformIr> {
    let mut indexed = Vec::new();
    if let Some(ops) = ops {
        for pair in ops.pairs::<i64, Table>() {
            let (idx, op_table) = pair?;
            let op_name: String = op_table.get("op").unwrap_or_default();
            let op = match op_name.as_str() {
                "translate" => TransformOpType::Translate,
                "rotate" => TransformOpType::Rotate,
                "scale" => TransformOpType::Scale,
                _ => continue,
            };
            indexed.push((
                idx,
                TransformOp {
                    op,
                    x: op_table.get("x").unwrap_or(0.0),
                    y: op_table.get("y").unwrap_or(0.0),
                    z: op_table.get("z").unwrap_or(0.0),
                },
            ));
        }
    }

    indexed.sort_by_key(|(idx, _)| *idx);
    let ops: Vec<TransformOp> = indexed.into_iter().map(|(_, op)| op).collect();
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
            matrix = mat_mul(op_matrix, matrix);
        }
        Self {
            matrix: round_matrix(matrix),
        }
    }
}

fn canonicalize_value(value: Value) -> Result<JsonValue> {
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
        Value::Error(e) => Ok(JsonValue::String(format!("<error:{e}>"))),
    }
}

fn canonicalize_table(table: Table) -> Result<JsonValue> {
    let mut array_entries: Vec<(i64, JsonValue)> = Vec::new();
    let mut map_entries: BTreeMap<String, JsonValue> = BTreeMap::new();
    let mut saw_non_array = false;

    for pair in table.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let canonical = canonicalize_value(value)?;

        match key {
            Value::Integer(i) if i > 0 => array_entries.push((i, canonical)),
            Value::Number(n) if n.fract() == 0.0 && n > 0.0 => {
                array_entries.push((n as i64, canonical))
            }
            Value::String(s) => {
                saw_non_array = true;
                map_entries.insert(s.to_str()?.to_string(), canonical);
            }
            _ => {
                saw_non_array = true;
                map_entries.insert("<unsupported_key>".to_string(), canonical);
            }
        }
    }

    array_entries.sort_by_key(|(idx, _)| *idx);
    let contiguous = !array_entries.is_empty()
        && array_entries
            .iter()
            .enumerate()
            .all(|(i, (idx, _))| *idx == i as i64 + 1);

    if contiguous && !saw_non_array {
        return Ok(JsonValue::Array(
            array_entries.into_iter().map(|(_, v)| v).collect(),
        ));
    }

    for (idx, value) in array_entries {
        map_entries.insert(idx.to_string(), value);
    }

    let mut map = Map::new();
    for (k, v) in map_entries {
        map.insert(k, v);
    }
    Ok(JsonValue::Object(map))
}

fn round_f64(value: f64) -> f64 {
    let scaled = (value * 1_000_000_000.0).round() / 1_000_000_000.0;
    if scaled == -0.0 {
        0.0
    } else {
        scaled
    }
}

fn round_matrix(mut matrix: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    for row in &mut matrix {
        for v in row {
            *v = round_f64(*v);
        }
    }
    matrix
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

fn rotation_zyx_matrix(rx_deg: f64, ry_deg: f64, rz_deg: f64) -> [[f64; 4]; 4] {
    let rx = rx_deg.to_radians();
    let ry = ry_deg.to_radians();
    let rz = rz_deg.to_radians();

    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();

    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz, 0.0],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz, 0.0],
        [-sy, sx * cy, cx * cy, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn mat_mul(a: [[f64; 4]; 4], b: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            out[r][c] = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}
