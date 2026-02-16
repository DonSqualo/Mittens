use anyhow::{anyhow, Result};
use mlua::{Lua, Table, Value};
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<TransformOpIr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ObjectIr>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransformOpIr {
    pub op: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
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

    let ops = canonicalize_ops(table.get::<_, Table>("ops").ok())?;

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
        ops,
        material,
        color,
        children,
    })
}

pub fn object_to_lua_table<'lua>(lua: &'lua Lua, obj: &ObjectIr) -> Result<Table<'lua>> {
    let t = lua.create_table()?;
    t.set("type", obj.obj_type.clone())?;

    if let Some(name) = &obj.name {
        t.set("name", name.clone())?;
    }
    if let Some(operation) = &obj.operation {
        t.set("operation", operation.clone())?;
    }
    if let Some(component) = &obj.component {
        t.set("component", component.clone())?;
    }
    if let Some(params) = &obj.params {
        t.set("params", json_to_lua_value(lua, params)?)?;
    }
    if !obj.ops.is_empty() {
        let ops = lua.create_table()?;
        for (i, op) in obj.ops.iter().enumerate() {
            let ot = lua.create_table()?;
            ot.set("op", op.op.clone())?;
            ot.set("x", op.x)?;
            ot.set("y", op.y)?;
            ot.set("z", op.z)?;
            ops.set((i + 1) as i64, ot)?;
        }
        t.set("ops", ops)?;
    }
    if let Some(material) = &obj.material {
        t.set("material", json_to_lua_value(lua, material)?)?;
    }
    if let Some(color) = &obj.color {
        t.set("color", json_to_lua_value(lua, color)?)?;
    }
    if !obj.children.is_empty() {
        let children = lua.create_table()?;
        for (i, child) in obj.children.iter().enumerate() {
            children.set((i + 1) as i64, object_to_lua_table(lua, child)?)?;
        }
        t.set("children", children)?;
    }

    Ok(t)
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

fn canonicalize_ops(ops: Option<Table>) -> Result<Vec<TransformOpIr>> {
    let mut indexed = Vec::new();
    if let Some(ops) = ops {
        for pair in ops.pairs::<i64, Table>() {
            let (idx, op_table) = pair?;
            let op: String = op_table.get("op").unwrap_or_default();
            if op != "translate" && op != "rotate" && op != "scale" {
                continue;
            }
            indexed.push((
                idx,
                TransformOpIr {
                    op,
                    x: round_f64(op_table.get("x").unwrap_or(0.0)),
                    y: round_f64(op_table.get("y").unwrap_or(0.0)),
                    z: round_f64(op_table.get("z").unwrap_or(0.0)),
                },
            ));
        }
    }

    indexed.sort_by_key(|(idx, _)| *idx);
    Ok(indexed.into_iter().map(|(_, op)| op).collect())
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
            Value::Number(n) if n.fract() == 0.0 && n > 0.0 => array_entries.push((n as i64, canonical)),
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

fn round_f64(value: f64) -> f64 {
    let scaled = (value * 1_000_000_000.0).round() / 1_000_000_000.0;
    if scaled == -0.0 {
        0.0
    } else {
        scaled
    }
}
