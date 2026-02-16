#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR/server"

echo "[prek] Running IR unit tests"
cargo test --release --bin ir_subagent

echo "[prek] Running STL regression tests (auto-runs when /home/heim/projects exists)"
cargo test --release --test project_stl_regression -- --nocapture
