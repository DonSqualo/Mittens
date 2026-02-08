# Router Service

This is an additive routing layer for many-renderer/many-server setups.

Strict behavior:
- Renderer must use `?backend_id=<id>`.
- Renderer connects only to `/ws/<id>`.
- No fallback `/ws` route is used by renderer.
- Router resolves `<id>` from one single runtime registry file.

## Single Source Of Truth

Registry file:
- `~/.mittens/backends.json`

Override path:
- set `MITTENS_REGISTRY_PATH` if needed

No per-branch config files. No per-agent config files.

Project file policy:
- backend project files must resolve under `~/projects` (enforced by `mittens backend start`)
- files under repo `examples/` are rejected by CLI

## Registry Schema

```json
{
  "updated_at_unix_ms": 0,
  "backends": [
    {
      "backend_id": "a1",
      "ws_url": "ws://127.0.0.1:4201/ws",
      "project_file": "/home/heim/projects/helmholtz/helmholtz_coil.lua",
      "branch": "codex/feature-x",
      "owner": "codex-a",
      "worktree": "/home/heim/Private_Mittens/codex-a",
      "backend_port": 4201,
      "pid": 12345,
      "updated_at_unix_ms": 0
    }
  ]
}
```

## Endpoints

- `GET /api/backends`
- `GET /ws/<backend_id>`
- `GET /healthz`

## Binaries

- `server/src/bin/router_service.rs`
- `server/src/bin/backend_registry.rs`
- `mittens` (CLI wrapper for router/backends/nginx)

## Commands

Initialize registry:

```bash
cd /home/heim/Private_Mittens/codex/server
MITTENS_REGISTRY_PATH="$HOME/.mittens/backends.json" \
cargo run --no-default-features --bin backend_registry -- init
```

Upsert backend entry:

```bash
cd /home/heim/Private_Mittens/codex/server
MITTENS_REGISTRY_PATH="$HOME/.mittens/backends.json" \
cargo run --no-default-features --bin backend_registry -- upsert \
  --backend-id a1 \
  --ws-url ws://127.0.0.1:4201/ws \
  --project-file /home/heim/projects/helmholtz/helmholtz_coil.lua \
  --branch codex/feature-a1 \
  --owner codex-a \
  --worktree /home/heim/Private_Mittens/codex-a1
```

Remove backend entry:

```bash
cd /home/heim/Private_Mittens/codex/server
MITTENS_REGISTRY_PATH="$HOME/.mittens/backends.json" \
cargo run --no-default-features --bin backend_registry -- remove \
  --backend-id a1
```

List registry:

```bash
cd /home/heim/Private_Mittens/codex/server
MITTENS_REGISTRY_PATH="$HOME/.mittens/backends.json" \
cargo run --no-default-features --bin backend_registry -- list
```

Run router service:

```bash
cd /home/heim/Private_Mittens/codex/server
MITTENS_REGISTRY_PATH="$HOME/.mittens/backends.json" \
MITTENS_ROUTER_PORT=3100 \
cargo run --no-default-features --bin router_service
```

## Renderer Usage

Required:
- `http://host/<existing-path>/?backend_id=a1`

This connects to:
- `/ws/a1`

No `backend_id`, no connection.

## Nginx Example

```nginx
location /ws/ {
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_pass http://127.0.0.1:3100;
}
```

## CLI Usage

Router:

```bash
mittens router start --registry "$HOME/.mittens/backends.json" --port 3100
mittens router status --port 3100
mittens router stop
```

Backends:

```bash
mittens backend start \
  --backend-id a1 \
  --project-file /home/heim/projects/helmholtz/helmholtz_coil.lua \
  --backend-port 4201 \
  --worktree /home/heim/Private_Mittens/codex \
  --registry "$HOME/.mittens/backends.json"

mittens backend status --backend-id a1 --registry "$HOME/.mittens/backends.json"
mittens backend stop --backend-id a1 --registry "$HOME/.mittens/backends.json"
mittens backend list --registry "$HOME/.mittens/backends.json"
```

Nginx snippet generation:

```bash
mittens nginx snippet --router-port 3100 --output /tmp/mittens-router.nginx.conf
```

## Persistent Runtime (Recommended)

Use systemd user services for auto-restart:

```bash
mittens systemd router install --port 3100
mittens systemd backend install \
  --backend-id pure-acoustics \
  --project-file /home/heim/projects/pure-acoustics/pure_acoustics.lua \
  --worktree /home/heim/Private_Mittens/codex

mittens systemd router status
mittens systemd backend status --backend-id pure-acoustics
```

These units run with `Restart=always`, so backend/router recover from crashes and session exits.
