use anyhow::{anyhow, Result};
use clap::Parser;
use mlua::{Lua, Table, Value};
use serde::Serialize;
use serde_json::{Map, Value as J};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "ir_snapshot")]
#[command(about = "Canonical Lua scene IR snapshot + stable hash")]
struct Cli {
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Serialize)]
struct SceneIr {
    kind: String,
    objects: Vec<ObjectIr>,
}

#[derive(Clone, Serialize)]
struct ObjectIr {
    #[serde(rename = "type")]
    obj_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<J>,
    transform: [[f64; 4]; 4],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    children: Vec<ObjectIr>,
}

#[derive(Serialize)]
struct Snapshot {
    file: String,
    scene_hash: String,
    object_count: usize,
    ir: SceneIr,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let lua = Lua::new();
    configure_package_path(&lua)?;

    let src = fs::read_to_string(&cli.file)?;
    let value: Value = lua.load(&src).eval()?;
    let scene = value
        .as_table()
        .ok_or_else(|| anyhow!("script must return scene table"))?;
    let objects = scene
        .get::<_, Table>("objects")
        .map_err(|_| anyhow!("scene missing objects"))?;

    let mut nodes = Vec::new();
    for p in objects.pairs::<i64, Table>() {
        let (_, t) = p?;
        nodes.push(canon_obj(&t)?);
    }
    nodes.sort_by_key(|o| hash_hex(&serde_json::to_string(o).unwrap_or_default()));

    let ir = SceneIr {
        kind: "scene".to_string(),
        objects: nodes,
    };
    let scene_hash = hash_hex(&serde_json::to_string(&ir)?);

    let snap = Snapshot {
        file: cli.file.display().to_string(),
        scene_hash,
        object_count: ir.objects.len(),
        ir,
    };

    let text = serde_json::to_string_pretty(&snap)?;
    if let Some(path) = cli.out {
        fs::write(path, &text)?;
    }
    println!("{}", text);
    Ok(())
}

fn canon_obj(t: &Table) -> Result<ObjectIr> {
    let obj_type: String = t.get("type")?;
    let operation = t.get::<_, String>("operation").ok().filter(|s| !s.is_empty());
    let component = t.get::<_, String>("component").ok().filter(|s| !s.is_empty());
    let name = t.get::<_, String>("name").ok().filter(|s| !s.is_empty());
    let params = t
        .get::<_, Table>("params")
        .ok()
        .map(canon_table)
        .transpose()?;

    let mut children = Vec::new();
    if let Ok(ct) = t.get::<_, Table>("children") {
        for p in ct.pairs::<i64, Table>() {
            let (_, c) = p?;
            children.push(canon_obj(&c)?);
        }
        if obj_type == "csg" && (operation.as_deref() == Some("union") || operation.as_deref() == Some("intersect")) {
            children.sort_by_key(|o| serde_json::to_string(o).unwrap_or_default());
        }
    }

    Ok(ObjectIr {
        obj_type,
        operation,
        component,
        name,
        params,
        transform: ops_to_matrix(t.get::<_, Table>("ops").ok().as_ref()),
        children,
    })
}

fn ops_to_matrix(ops: Option<&Table>) -> [[f64; 4]; 4] {
    let mut m = identity();
    if let Some(ops) = ops {
        let mut list: Vec<(i64, String, f64, f64, f64)> = Vec::new();
        for (idx, op) in ops.clone().pairs::<i64, Table>().flatten() {
            let name: String = op.get("op").unwrap_or_default();
            list.push((
                idx,
                name,
                op.get("x").unwrap_or(0.0),
                op.get("y").unwrap_or(0.0),
                op.get("z").unwrap_or(0.0),
            ));
        }
        list.sort_by_key(|x| x.0);
        for (_, op, x, y, z) in list {
            let t = match op.as_str() {
                "translate" => [[1.0, 0.0, 0.0, x], [0.0, 1.0, 0.0, y], [0.0, 0.0, 1.0, z], [0.0, 0.0, 0.0, 1.0]],
                "scale" => [[x, 0.0, 0.0, 0.0], [0.0, y, 0.0, 0.0], [0.0, 0.0, z, 0.0], [0.0, 0.0, 0.0, 1.0]],
                "rotate" => rot_zyx(x, y, z),
                _ => continue,
            };
            m = mat_mul(t, m);
        }
    }
    round_mat(m)
}

fn rot_zyx(rx_deg: f64, ry_deg: f64, rz_deg: f64) -> [[f64; 4]; 4] {
    let (rx, ry, rz) = (rx_deg.to_radians(), ry_deg.to_radians(), rz_deg.to_radians());
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let rxm = [[1.0, 0.0, 0.0, 0.0], [0.0, cx, -sx, 0.0], [0.0, sx, cx, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let rym = [[cy, 0.0, sy, 0.0], [0.0, 1.0, 0.0, 0.0], [-sy, 0.0, cy, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let rzm = [[cz, -sz, 0.0, 0.0], [sz, cz, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    mat_mul(rzm, mat_mul(rym, rxm))
}

fn canon_table(t: Table) -> Result<J> {
    let mut arr: Vec<(i64, J)> = Vec::new();
    let mut map: BTreeMap<String, J> = BTreeMap::new();
    let mut saw_map = false;

    for p in t.pairs::<Value, Value>() {
        let (k, v) = p?;
        let cv = canon_val(v)?;
        match k {
            Value::Integer(i) if i > 0 => arr.push((i, cv)),
            Value::Number(n) if n.fract() == 0.0 && n > 0.0 => arr.push((n as i64, cv)),
            Value::String(s) => {
                saw_map = true;
                map.insert(s.to_str()?.to_string(), cv);
            }
            _ => {
                saw_map = true;
                map.insert(format!("{:?}", k.type_name()), cv);
            }
        }
    }

    arr.sort_by_key(|x| x.0);
    let contiguous = !arr.is_empty() && arr.iter().enumerate().all(|(i, (k, _))| *k == i as i64 + 1);
    if contiguous && !saw_map {
        return Ok(J::Array(arr.into_iter().map(|(_, v)| v).collect()));
    }
    for (k, v) in arr {
        map.insert(k.to_string(), v);
    }
    Ok(J::Object(map.into_iter().collect::<Map<String, J>>()))
}

fn canon_val(v: Value) -> Result<J> {
    Ok(match v {
        Value::Nil => J::Null,
        Value::Boolean(b) => J::Bool(b),
        Value::Integer(i) => J::from(i),
        Value::Number(n) => J::from(round(n)),
        Value::String(s) => J::String(s.to_str()?.to_string()),
        Value::Table(t) => canon_table(t)?,
        _ => J::String(format!("<{}>", v.type_name())),
    })
}

fn round(v: f64) -> f64 {
    let r = (v * 1e9).round() / 1e9;
    if r == -0.0 { 0.0 } else { r }
}

fn round_mat(mut m: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    for row in &mut m {
        for v in row {
            *v = round(*v);
        }
    }
    m
}

fn identity() -> [[f64; 4]; 4] {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

fn mat_mul(a: [[f64; 4]; 4], b: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut o = [[0.0; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            o[r][c] = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    o
}

fn hash_hex(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn configure_package_path(lua: &Lua) -> Result<()> {
    let package: Table = lua.globals().get("package")?;
    let existing: String = package.get("path").unwrap_or_default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow!("failed to resolve repo root"))?;
    let p = format!("{};{};{}", root.join("?.lua").display(), root.join("?/init.lua").display(), existing);
    package.set("path", p)?;
    Ok(())
}
