#!/usr/bin/env bash
# Runs the CI pipeline locally (single OS). Usage: scripts/ci-local.sh [--no-fuzz]
set -euo pipefail
cd "$(dirname "$0")/.."
export RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings"
step() { echo; echo "=== $*"; "$@"; }
step cargo fmt --all --check
step cargo clippy --workspace --all-targets --all-features -- -D warnings
step scripts/debt-check.sh
step scripts/verify-vendor.sh
step python3 scripts/gen_tables.py --check
step cargo doc --workspace --no-deps --all-features
step cargo deny check
step cargo test --workspace --all-features
step cargo test -p expat
step cargo test -p expat --no-default-features
MIRIFLAGS=-Zmiri-strict-provenance step cargo +nightly miri test -p expat --all-features
if [ "${1:-}" != "--no-fuzz" ]; then step scripts/fuzz-smoke.sh 60; fi
echo; echo "ci-local: ALL GREEN"
