# AGENT.md

## Preferred Startup Path

Use `mittens init PROJECT.lua` as the default way to bring up a project.

Example:

```bash
./mittens init tube.lua --projects-root "$PWD/examples" --worktree "$PWD"
```

What `init` does:

1. Resolves the Lua project file.
2. Installs/starts router via user systemd.
3. Installs/starts renderer via user systemd.
4. Installs/starts backend via user systemd.
5. Ensures Manifold runtime libraries are in backend unit `LD_LIBRARY_PATH`.
6. Regenerates nginx snippet.
7. Enables/reloads nginx service (unless `--no-nginx` is passed).

## Operational Rule

Avoid manually composing `router on` + `server on` + `renderer on` for normal bring-up.
Use `init` so environments are reproducible and graph discovery is stable.
