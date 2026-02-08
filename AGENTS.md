# AGENTS.md

## CRITICAL RULE: `SERVE X`

When user says `SERVE X`, always do exactly this:

1. Resolve `X` to a Lua project file under `~/projects`.
2. Use `mittens` CLI to serve that project file.

Never substitute repo `examples/` for `SERVE X` unless the user explicitly asks for examples.
