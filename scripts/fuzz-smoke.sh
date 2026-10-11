#!/usr/bin/env bash
# Runs every fuzz target for N seconds (default 60) on its committed corpus.
# Any crash fails the script. Usage: scripts/fuzz-smoke.sh [seconds]
set -euo pipefail
cd "$(dirname "$0")/../fuzz"
secs=${1:-60}
for t in $(cargo +nightly fuzz list); do
  mkdir -p "corpus/$t"
  echo "== $t ($secs s)"
  cargo +nightly fuzz run "$t" "corpus/$t" -- -max_total_time="$secs" -max_len=65536 -rss_limit_mb=2048 2>&1 | tail -n 3
done
