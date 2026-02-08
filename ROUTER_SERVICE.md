# Router Service

Mittens supports multiple backends and renderers through a single router.

## Behavior

- Renderer must include `?backend_id=<id>`.
- Renderer connects to `/ws/<backend_id>`.
- Router resolves backend metadata from one registry file.

## Registry

- Default path: `$HOME/.mittens/backends.json`
- Override: `MITTENS_REGISTRY_PATH`

Example entry:

```json
{
  "backend_id": "a1",
  "ws_url": "ws://127.0.0.1:4201/ws",
  "project_file": "/abs/path/to/project.lua",
  "branch": "codex/feature-a1",
  "owner": "dev-a",
  "worktree": "/abs/path/to/worktree",
  "backend_port": 4201,
  "pid": 12345,
  "updated_at_unix_ms": 0
}
```

## Endpoints

- `GET /api/backends`
- `GET /api/graph`
- `GET /graph`
- `GET /ws/<backend_id>`
- `GET /healthz`

## CLI Quick Usage

From repo root:

```bash
./mittens router start --registry "$HOME/.mittens/backends.json" --port 3100

./mittens backend start \
  --backend-id a1 \
  --project-file examples/pure_acoustics.lua \
  --projects-root "$PWD" \
  --backend-port 4201 \
  --worktree "$PWD" \
  --registry "$HOME/.mittens/backends.json"

./mittens run renderer \
  --renderer-id local \
  --worktree "$PWD" \
  --port 3000 \
  --host 0.0.0.0
```

Open:

- `http://localhost:3000/?backend_id=a1`

## Low-Level Binaries

- `server/src/bin/router_service.rs`
- `server/src/bin/backend_registry.rs`

Initialize registry manually:

```bash
cd server
MITTENS_REGISTRY_PATH="$HOME/.mittens/backends.json" \
cargo run --no-default-features --bin backend_registry -- init
```

Run router manually:

```bash
cd server
MITTENS_REGISTRY_PATH="$HOME/.mittens/backends.json" \
MITTENS_ROUTER_PORT=3100 \
cargo run --no-default-features --bin router_service
```

## Nginx Example

```nginx
location /ws/ {
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_pass http://127.0.0.1:3100;
}
```
