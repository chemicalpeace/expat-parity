#!/usr/bin/env bash
# Coverage of crates/expat with cargo-llvm-cov over the FULL test suite (the
# same two commands as the CI test job, nothing skipped), instrumented
# release build. Writes per-run reports into OUT_DIR:
#
#   <run>-summary.txt         llvm-cov report table (crates/expat files only)
#   <run>-summary.json        the same as JSON (--summary-only)
#   <run>.lcov                lcov export
#   <run>-missing-lines.txt   the table plus the uncovered lines per file
#   <run>-mismatch.txt        llvm-cov's "mismatched data" warning with the
#                             function list (llvm-cov -dump hash-mismatch lines)
#
# Runs: "all-features" = `cargo test --workspace --all-features`;
#       "full"         = all-features plus `cargo test -p expat --no-default-features`.
#
# Usage: scripts/coverage.sh OUT_DIR [--doctests]
#   --doctests uses the nightly toolchain (cargo-llvm-cov's --doctests is
#   nightly-only) and adds the doctests to both runs.
set -euo pipefail
cd "$(dirname "$0")/.."

out=${1:?usage: scripts/coverage.sh OUT_DIR [--doctests]}
mkdir -p "$out"
out=$(cd "$out" && pwd)

tc=()
doc=()
if [ "${2:-}" = "--doctests" ]; then
  tc=(+nightly)
  doc=(--doctests)
fi

# Same filter as docs/REPORT.md module 11 step E, written for / and \ paths:
# the harness crate (tests/), the C oracle, benches, fuzz targets, vendored C.
ignore='(tests[/\\]|crates[/\\]expat-oracle|benches[/\\]|fuzz[/\\]|vendor)'

cov() { cargo ${tc[@]+"${tc[@]}"} llvm-cov "$@"; }

report() {
  local run=$1
  cov report --release ${doc[@]+"${doc[@]}"} --ignore-filename-regex "$ignore" \
    > "$out/$run-summary.txt"
  cov report --release ${doc[@]+"${doc[@]}"} --ignore-filename-regex "$ignore" \
    --show-missing-lines > "$out/$run-missing-lines.txt"
  cov report --release ${doc[@]+"${doc[@]}"} --ignore-filename-regex "$ignore" \
    --json --summary-only --output-path "$out/$run-summary.json"
  cov report --release ${doc[@]+"${doc[@]}"} --ignore-filename-regex "$ignore" \
    --lcov --output-path "$out/$run.lcov"
  # -dump makes llvm-cov list every hash mismatch on stderr.
  LLVM_COV_FLAGS=-dump cov report --release ${doc[@]+"${doc[@]}"} \
    --ignore-filename-regex "$ignore" > /dev/null 2> "$out/$run-dump.txt"
  { grep -E '^warning: ' "$out/$run-dump.txt" || echo 'no warnings'
    grep -E '^hash-mismatch: ' "$out/$run-dump.txt" || true
  } > "$out/$run-mismatch.txt"
  rm "$out/$run-dump.txt"
  echo "== $run"
  tail -n 1 "$out/$run-summary.txt"
  head -n 1 "$out/$run-mismatch.txt"
}

{
  cargo ${tc[@]+"${tc[@]}"} --version
  cargo ${tc[@]+"${tc[@]}"} llvm-cov --version
  rustc ${tc[@]+"${tc[@]}"} -vV
} > "$out/versions.txt"

cov clean --workspace
cov --no-report --release ${doc[@]+"${doc[@]}"} --workspace --all-features
report all-features
cov --no-report --release ${doc[@]+"${doc[@]}"} -p expat --no-default-features
report full
