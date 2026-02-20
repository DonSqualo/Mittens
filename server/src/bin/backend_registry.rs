use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendEntry {
    backend_id: String,
    ws_url: String,
    project_file: Option<String>,
    branch: Option<String>,
    owner: Option<String>,
    worktree: Option<String>,
    backend_port: Option<u16>,
    pid: Option<u32>,
    updated_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendRegistry {
    updated_at_unix_ms: u64,
    backends: Vec<BackendEntry>,
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args
        .next()
        .ok_or_else(|| anyhow!("missing command (init|list|get|field|upsert|remove)"))?;
    let parsed = parse_flag_args(args.collect::<Vec<_>>())?;

    let registry_path = resolve_registry_path();
    ensure_parent_dir(&registry_path)?;

    match command.as_str() {
        "init" => {
            with_registry_lock(&registry_path, || {
                let registry = load_registry_or_default(&registry_path)?;
                save_registry_atomic(&registry_path, &registry)
            })?;
            println!("{}", registry_path.display());
        }
        "list" => {
            let registry = load_registry_or_default(&registry_path)?;
            println!("{}", serde_json::to_string_pretty(&registry)?);
        }
        "get" => {
            let backend_id = required_arg(&parsed, "--backend-id")?;
            let registry = load_registry_or_default(&registry_path)?;
            let backend = get_backend(&registry.backends, &backend_id)
                .ok_or_else(|| anyhow!("backend '{}' not found", backend_id))?;
            println!("{}", serde_json::to_string_pretty(backend)?);
        }
        "field" => {
            let backend_id = required_arg(&parsed, "--backend-id")?;
            let field_name = required_arg(&parsed, "--name")?;
            let registry = load_registry_or_default(&registry_path)?;
            let backend = get_backend(&registry.backends, &backend_id)
                .ok_or_else(|| anyhow!("backend '{}' not found", backend_id))?;
            let value = get_field_value(backend, &field_name)
                .ok_or_else(|| anyhow!("field '{}' is not set for backend '{}'", field_name, backend_id))?;
            println!("{}", value);
        }
        "upsert" => {
            let backend_id = required_arg(&parsed, "--backend-id")?;
            let ws_url = required_arg(&parsed, "--ws-url")?;
            let project_file = optional_arg(&parsed, "--project-file");
            let branch = optional_arg(&parsed, "--branch");
            let owner = optional_arg(&parsed, "--owner");
            let worktree = optional_arg(&parsed, "--worktree");
            let backend_port = optional_arg(&parsed, "--backend-port")
                .map(|v| parse_u16_arg("--backend-port", &v))
                .transpose()?;
            let pid = optional_arg(&parsed, "--pid")
                .map(|v| parse_u32_arg("--pid", &v))
                .transpose()?;

            let now = now_unix_ms();
            let entry = BackendEntry {
                backend_id: backend_id.clone(),
                ws_url,
                project_file,
                branch,
                owner,
                worktree,
                backend_port,
                pid,
                updated_at_unix_ms: Some(now),
            };

            with_registry_lock(&registry_path, || {
                let mut registry = load_registry_or_default(&registry_path)?;
                upsert_backend(&mut registry.backends, entry);
                registry.updated_at_unix_ms = now;
                save_registry_atomic(&registry_path, &registry)
            })?;
            println!("upserted {}", backend_id);
        }
        "remove" => {
            let backend_id = required_arg(&parsed, "--backend-id")?;
            let now = now_unix_ms();
            with_registry_lock(&registry_path, || {
                let mut registry = load_registry_or_default(&registry_path)?;
                registry
                    .backends
                    .retain(|entry| entry.backend_id != backend_id);
                registry.updated_at_unix_ms = now;
                save_registry_atomic(&registry_path, &registry)
            })?;
            println!("removed {}", backend_id);
        }
        _ => {
            return Err(anyhow!("unknown command '{}'", command));
        }
    }

    Ok(())
}

fn resolve_registry_path() -> PathBuf {
    if let Ok(path) = env::var("MITTENS_REGISTRY_PATH") {
        return PathBuf::from(path);
    }
    default_registry_path()
}

fn default_registry_path() -> PathBuf {
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".mittens").join("backends.json");
    }
    PathBuf::from(".mittens/backends.json")
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("invalid registry path '{}'", path.display()))?;
    fs::create_dir_all(parent)?;
    Ok(())
}

fn required_arg(args: &HashMap<String, String>, name: &str) -> Result<String> {
    args.get(name)
        .cloned()
        .ok_or_else(|| anyhow!("missing argument {}", name))
}

fn optional_arg(args: &HashMap<String, String>, name: &str) -> Option<String> {
    args.get(name).cloned()
}

fn load_registry_or_default(path: &Path) -> Result<BackendRegistry> {
    match fs::read(path) {
        Ok(bytes) => {
            let mut registry = serde_json::from_slice::<BackendRegistry>(&bytes)?;
            registry
                .backends
                .sort_by(|a, b| a.backend_id.cmp(&b.backend_id));
            Ok(registry)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(BackendRegistry {
            updated_at_unix_ms: now_unix_ms(),
            backends: Vec::new(),
        }),
        Err(err) => Err(err.into()),
    }
}

fn save_registry_atomic(path: &Path, registry: &BackendRegistry) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("invalid registry path '{}'", path.display()))?;
    let temp_name = format!(
        ".backends.json.tmp.{}.{}",
        std::process::id(),
        now_unix_ms()
    );
    let temp_path = parent.join(temp_name);
    let bytes = serde_json::to_vec_pretty(registry)?;
    fs::write(&temp_path, bytes)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

fn upsert_backend(backends: &mut Vec<BackendEntry>, entry: BackendEntry) {
    if let Some(found) = backends
        .iter_mut()
        .find(|existing| existing.backend_id == entry.backend_id)
    {
        *found = entry;
    } else {
        backends.push(entry);
    }

    backends.sort_by(|a, b| a.backend_id.cmp(&b.backend_id));
}

fn get_backend<'a>(backends: &'a [BackendEntry], backend_id: &str) -> Option<&'a BackendEntry> {
    backends.iter().find(|entry| entry.backend_id == backend_id)
}

fn get_field_value(entry: &BackendEntry, field_name: &str) -> Option<String> {
    match field_name {
        "backend_id" => Some(entry.backend_id.clone()),
        "ws_url" => Some(entry.ws_url.clone()),
        "project_file" => entry.project_file.clone(),
        "branch" => entry.branch.clone(),
        "owner" => entry.owner.clone(),
        "worktree" => entry.worktree.clone(),
        "backend_port" => entry.backend_port.map(|v| v.to_string()),
        "pid" => entry.pid.map(|v| v.to_string()),
        "updated_at_unix_ms" => entry.updated_at_unix_ms.map(|v| v.to_string()),
        _ => None,
    }
}

fn with_registry_lock<T>(registry_path: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let lock_path = registry_path.with_extension("lock");
    let mut attempts = 0usize;
    loop {
        let lock = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path);
        match lock {
            Ok(file) => {
                drop(file);
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists && attempts < 100 => {
                attempts += 1;
                thread::sleep(Duration::from_millis(25));
            }
            Err(err) => return Err(err.into()),
        }
    }

    let result = f();
    let _ = fs::remove_file(lock_path);
    result
}

fn parse_u16_arg(name: &str, value: &str) -> Result<u16> {
    value
        .parse::<u16>()
        .map_err(|e| anyhow!("invalid {} '{}': {}", name, value, e))
}

fn parse_u32_arg(name: &str, value: &str) -> Result<u32> {
    value
        .parse::<u32>()
        .map_err(|e| anyhow!("invalid {} '{}': {}", name, value, e))
}

fn parse_flag_args(args: Vec<String>) -> Result<HashMap<String, String>> {
    if args.len() % 2 != 0 {
        return Err(anyhow!("arguments must be '--key value' pairs"));
    }

    let mut map = HashMap::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = &args[i + 1];
        if !key.starts_with("--") {
            return Err(anyhow!("invalid argument key '{}'", key));
        }
        map.insert(key.clone(), value.clone());
        i += 2;
    }
    Ok(map)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> BackendEntry {
        BackendEntry {
            backend_id: id.to_string(),
            ws_url: format!("ws://{id}"),
            project_file: None,
            branch: None,
            owner: None,
            worktree: None,
            backend_port: None,
            pid: None,
            updated_at_unix_ms: Some(123),
        }
    }

    #[test]
    fn upsert_backend_inserts_and_keeps_sorted_order() {
        let mut backends = vec![entry("zeta"), entry("beta")];

        upsert_backend(&mut backends, entry("alpha"));

        let ids: Vec<&str> = backends.iter().map(|b| b.backend_id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "beta", "zeta"]);
    }

    #[test]
    fn upsert_backend_updates_existing_entry_without_duplicates() {
        let mut backends = vec![entry("alpha"), entry("beta")];

        let mut updated = entry("beta");
        updated.ws_url = "ws://new-beta".to_string();
        updated.backend_port = Some(3100);

        upsert_backend(&mut backends, updated);

        assert_eq!(backends.len(), 2);
        let beta = get_backend(&backends, "beta").expect("beta backend should exist");
        assert_eq!(beta.ws_url, "ws://new-beta");
        assert_eq!(beta.backend_port, Some(3100));
    }

    #[test]
    fn parse_flag_args_rejects_non_flag_keys() {
        let args = vec!["backend-id".to_string(), "abc".to_string()];
        let err = parse_flag_args(args).expect_err("expected parse failure");
        assert!(err.to_string().contains("invalid argument key"));
    }

    #[test]
    fn parse_flag_args_requires_even_pairs() {
        let args = vec!["--backend-id".to_string(), "abc".to_string(), "--ws-url".to_string()];
        let err = parse_flag_args(args).expect_err("expected parse failure");
        assert!(err
            .to_string()
            .contains("arguments must be '--key value' pairs"));
    }
}
