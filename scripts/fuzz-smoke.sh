#!/usr/bin/env bash
# Runs every fuzz target for N seconds (default 60) on its committed corpus
# (fuzz/corpus/<target>, docs/FUZZING.md). A target without a committed,
# non-empty corpus fails the script, as does any crash.
# Usage: scripts/fuzz-smoke.sh [seconds]
set -euo pipefail
cd "$(dirname "$0")/../fuzz"
secs=${1:-60}
for t in $(cargo +nightly fuzz list); do
  n=$(find "corpus/$t" -type f 2>/dev/null | wc -l)
  if [ "$n" -eq 0 ]; then
    echo "error: fuzz target $t has no committed corpus in fuzz/corpus/$t" >&2
    exit 1
  fi
  echo "== $t ($secs s, $n committed corpus files)"
  cargo +nightly fuzz run "$t" "corpus/$t" -- -max_total_time="$secs" -max_len=65536 -rss_limit_mb=2048 2>&1 | tail -n 3
done
