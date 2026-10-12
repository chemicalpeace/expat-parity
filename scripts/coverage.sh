#!/usr/bin/env bash
# Coverage of crates/expat with cargo-llvm-cov over the FULL test suite (the
# same two commands as the CI test job, nothing skipped), instrumented
# release build. Writes per-run reports into OUT_DIR:
#
#   <run>-summary.txt         llvm-cov report table (crates/expat files only)
#   <run>-summary.json        the same as JSON (--summary-only)
#   <run>.lcov                lcov export
#   <run>-missing-lines.txt   the table plus the uncovered lines per file
#   <run>-warnings.txt        llvm-cov's warnings (e.g. "N functions have
#                             mismatched data")
#   <run>-mismatch.txt        stable toolchain only: every mismatched
#                             function (llvm-cov -dump "hash-mismatch" lines)
#
# Each run is reported twice: "<run>-raw-*" is cargo-llvm-cov's report as it
# comes; "<run>-*" adds the profile records of scripts/coverage_fix_profdata.py,
# which stop llvm-cov from dropping the counts of functions that some test
# binary links without using (see that script). The corrected report is the
# one docs/REPORT.md quotes.
#
# Runs: "all-features" = `cargo test --workspace --all-features`;
#       "full"         = all-features plus `cargo test -p expat --no-default-features`.
#
# Usage: scripts/coverage.sh OUT_DIR [stable|nightly|nightly-doctests]
#   stable (default): the pinned toolchain of rust-toolchain.toml.
#   nightly: the same runs on nightly (control for nightly-doctests).
#   nightly-doctests: nightly plus the doctests (cargo-llvm-cov --doctests
#   needs nightly).
set -euo pipefail
cd "$(dirname "$0")/.."

out=${1:?usage: scripts/coverage.sh OUT_DIR [stable|nightly|nightly-doctests]}
mode=${2:-stable}
mkdir -p "$out"
out=$(cd "$out" && pwd)

tc=()
doc=()
case $mode in
  stable) ;;
  nightly) tc=(+nightly) ;;
  nightly-doctests) tc=(+nightly); doc=(--doctests) ;;
  *) echo "unknown mode $mode" >&2; exit 2 ;;
esac

# Same filter as docs/REPORT.md module 11 step E, written for / and \ paths:
# the harness crate (tests/), the C oracle, benches, fuzz targets, vendored C.
ignore='(tests[/\\]|crates[/\\]expat-oracle|benches[/\\]|fuzz[/\\]|vendor)'

cov() { cargo ${tc[@]+"${tc[@]}"} llvm-cov "$@"; }

host=$(rustc ${tc[@]+"${tc[@]}"} -vV | sed -n 's/^host: //p')
bin="$(rustc ${tc[@]+"${tc[@]}"} --print sysroot)/lib/rustlib/$host/bin"
profdata_tool="$bin/llvm-profdata"
[ -x "$profdata_tool" ] || profdata_tool="$bin/llvm-profdata.exe"
fix=target/llvm-cov-target/hash-fix.profraw
py=python3
"$py" -c 1 2>/dev/null || py=python

report() {
  local run=$1
  local args=(--release ${doc[@]+"${doc[@]}"} --ignore-filename-regex "$ignore")
  cov report "${args[@]}" > "$out/$run-summary.txt" 2> "$out/$run-warnings.txt"
  cov report "${args[@]}" --show-missing-lines > "$out/$run-missing-lines.txt"
  cov report "${args[@]}" --json --summary-only --output-path "$out/$run-summary.json"
  cov report "${args[@]}" --lcov --output-path "$out/$run.lcov"
  if [ "$mode" = stable ]; then
    # -dump makes llvm-cov list every hash mismatch on stderr. (The nightly
    # llvm-cov of 2026-10-11 aborts in -dump, so this is stable only.)
    LLVM_COV_FLAGS=-dump cov report "${args[@]}" > /dev/null 2> "$out/$run-dump.txt"
    # grep exits 1 when there is no mismatch at all; 2 (a real error) fails.
    grep -E '^hash-mismatch: ' "$out/$run-dump.txt" > "$out/$run-mismatch.txt" || [ $? -eq 1 ]
    rm "$out/$run-dump.txt"
  fi
  echo "== $mode $run"
  tail -n 1 "$out/$run-summary.txt"
  cat "$out/$run-warnings.txt"
}

{
  cargo ${tc[@]+"${tc[@]}"} --version
  cargo ${tc[@]+"${tc[@]}"} llvm-cov --version
  rustc ${tc[@]+"${tc[@]}"} -vV
} > "$out/versions.txt"

corrected() {
  local run=$1
  report "$run-raw"
  local profiles=(target/llvm-cov-target/*.profdata)
  [ ${#profiles[@]} -eq 1 ] || { echo "expected one .profdata: ${profiles[*]}" >&2; exit 1; }
  "$py" scripts/coverage_fix_profdata.py "$profdata_tool" "${profiles[0]}" "$fix" \
    | tee "$out/$run-fix.txt"
  report "$run"
  rm "$fix"
}

cov clean --workspace
cov --no-report --release ${doc[@]+"${doc[@]}"} --workspace --all-features
corrected all-features
cov --no-report --release ${doc[@]+"${doc[@]}"} -p expat --no-default-features
corrected full
