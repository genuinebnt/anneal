#!/usr/bin/env sh
# CI-style check for cb-backend-reference. fmt and clippy run only if already installed (nothing is
# installed by this script). Run from any directory.
set -eu
cd "$(dirname "$0")/.."

step() { printf '\n== %s\n' "$*"; }

if cargo fmt --version >/dev/null 2>&1; then
  step "rustfmt --check"
  cargo fmt -p cb-backend-reference -- --check
else
  step "rustfmt not installed: skipped"
fi

if cargo clippy --version >/dev/null 2>&1; then
  step "clippy (warnings are errors)"
  cargo clippy -p cb-backend-reference --all-targets -- -D warnings
else
  step "clippy not installed: skipped"
fi

step "tests"
cargo test -q --release -p cb-backend-reference

step "end-to-end demo"
cargo run -q --release -p cb-backend-reference --bin backend-demo >/dev/null

step "ok"
