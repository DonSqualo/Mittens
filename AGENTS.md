# AGENTS.md

## CRITICAL RULE: `SERVE X`

When user says `SERVE X`, always do exactly this:

1. Resolve `X` to a Lua project file under `~/projects`.
2. Use `mittens` CLI to serve that project file.

Never substitute repo `examples/` for `SERVE X` unless the user explicitly asks for examples.

## Codebase Notes

- Renderer source entrypoint is `renderer/src/main.ts`.
- For isolated renderer behavior tests, use pure helpers under `renderer/src/` and run tests with Node: `node --test --experimental-strip-types src/**/*.test.ts`.
- Renderer typecheck runs with `npm run typecheck` in `renderer/`.
- In this environment, `mittens server on` may need `LD_LIBRARY_PATH` including manifold build outputs under `server/target/debug/build/manifold3d-sys-*/out/build/{bindings/c,src}`.
