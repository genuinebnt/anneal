#!/usr/bin/env bash
# Build and run every cookbook example (recipes rNN_* and capstones cNN_*), each with its own time limit.
#   ./check.sh            all examples
#   ./check.sh r1         only examples whose file name starts with "r1"
#   CARGO_TARGET_DIR=/tmp/cb_target_shared ./check.sh     use a shared build directory
# An example may declare `//! EXPECT-EXIT: N` in its header when a non-zero exit is the demonstration.
# A std-only example may declare `//! OVERFLOW-CHECKS` when one of its checks needs them: release builds wrap on integer
# overflow, so it is built once more with `rustc -O -C overflow-checks=on` and run again.
# CHECK_LOG_DIR changes where the logs go.
# Each example is built separately, so one broken file never hides the others. Exit status is non-zero if anything failed.
cd "$(dirname "$0")" || exit 1
target_dir="${CARGO_TARGET_DIR:-target}"
limit="${CHECK_TIMEOUT:-90}"
logs="${CHECK_LOG_DIR:-/tmp/cookbook_logs}"
mkdir -p "$logs"
pass=0
fail=0
failed=()
for f in examples/*/${1:-}*.rs; do
  case "$f" in examples/scratch/*) continue;; esac
  n=$(basename "$f" .rs)
  if ! cargo build -q --release --example "$n" >"$logs/$n.build" 2>&1; then
    echo "FAIL  $n  (build error: $logs/$n.build)"
    fail=$((fail + 1)); failed+=("$n"); continue
  fi
  want=$(grep -o 'EXPECT-EXIT: [0-9]*' "$f" | head -1 | cut -d' ' -f2); want=${want:-0}
  perl -e "alarm $limit; exec @ARGV" "$target_dir/release/examples/$n" >"$logs/$n.out" 2>&1
  got=$?
  if [ "$got" = "$want" ]; then
    echo "ok    $n"; pass=$((pass + 1))
  else
    echo "FAIL  $n  exit=$got want=$want (see $logs/$n.out)"; fail=$((fail + 1)); failed+=("$n")
  fi
  if [ -s "$logs/$n.build" ]; then echo "warn  $n has compiler output ($logs/$n.build)"; fi
  if grep -q '^//! OVERFLOW-CHECKS' "$f"; then
    if rustc --edition 2024 -O -C overflow-checks=on --crate-name "$n" "$f" -o "$logs/$n.overflow-bin" >"$logs/$n.overflow" 2>&1 &&
      perl -e "alarm $limit; exec @ARGV" "$logs/$n.overflow-bin" >>"$logs/$n.overflow" 2>&1; then
      echo "ok    $n (overflow checks on)"; pass=$((pass + 1))
    else
      echo "FAIL  $n with overflow checks on (see $logs/$n.overflow)"; fail=$((fail + 1)); failed+=("$n:overflow")
    fi
  fi
done
# multi-crate recipes (crates/*): run their tests
for d in crates/*/; do
  [ -f "$d/Cargo.toml" ] || continue
  n=$(basename "$d")
  if cargo test -q --release -p "$(grep -m1 '^name' "$d/Cargo.toml" | cut -d'"' -f2)" >"$logs/crate_$n.out" 2>&1; then
    echo "ok    crates/$n"; pass=$((pass + 1))
  else
    echo "FAIL  crates/$n (see $logs/crate_$n.out)"; fail=$((fail + 1)); failed+=("crates/$n")
  fi
done
# unit tests that live inside examples (e.g. r49)
if cargo test -q --release --examples >"$logs/unit_tests.out" 2>&1; then
  echo "ok    (example unit tests)"
else
  echo "FAIL  example unit tests (see $logs/unit_tests.out)"; fail=$((fail + 1))
fi
echo "---- passed: $pass  failed: $fail ${failed[*]}"
[ "$fail" = 0 ]
