#!/usr/bin/env bash
# Repeats `llvm-cov report` on a small instrumented build and counts runs that
# do not exit 0 (docs/evidence/llvm-cov-dump-race/README.md).
#
#   stress.sh prepare                 instrumented build of two test targets,
#                                     merged profile in target/llvm-cov-target/race.profdata
#   stress.sh fixprof                 also writes race-fix.profdata: the same raw
#                                     profiles plus scripts/coverage_fix_profdata.py's records
#   stress.sh run LABEL SECS PROFILE [llvm-cov flags...]
#                                     repeats for SECS seconds; appends one result line
#                                     to $RESULTS (default /tmp/llvm-cov-race-results.txt) and
#                                     keeps the stderr of every failing run in $KEEP
set -uo pipefail
cd "$(dirname "$0")/../../.."
host=$(rustc -vV | sed -n 's/^host: //p')
bin="$(rustc --print sysroot)/lib/rustlib/$host/bin"
tgt=target/llvm-cov-target
case ${1:-} in
  prepare)
    cargo llvm-cov clean --workspace
    cargo llvm-cov --no-report --release -p expat-difftest --test entity_diff --test xmldecl_unknown_diff
    "$bin/llvm-profdata" merge -sparse $tgt/*.profraw -o $tgt/race.profdata ;;
  fixprof)
    python3 scripts/coverage_fix_profdata.py "$bin/llvm-profdata" $tgt/race.profdata /tmp/race-fix.proftext
    "$bin/llvm-profdata" merge -sparse $tgt/*.profraw /tmp/race-fix.proftext -o $tgt/race-fix.profdata ;;
  run)
    label=$2; secs=$3; prof=$4; shift 4
    objs=()
    for t in entity_diff xmldecl_unknown_diff; do
      f=$(ls -t $tgt/release/deps/$t-* | grep -v '\.d$' | head -1); objs+=(-object "$f")
    done
    results=${RESULTS:-/tmp/llvm-cov-race-results.txt}; keep=${KEEP:-/tmp/llvm-cov-race-failures}
    mkdir -p "$keep"
    end=$(( $(date +%s) + secs )); n=0; bad=0; codes=""
    while [ "$(date +%s)" -lt "$end" ]; do
      timeout 30 "$bin/llvm-cov" report "$@" -instr-profile="$tgt/$prof" "${objs[@]}" \
        -ignore-filename-regex '(tests/|expat-oracle|vendor|/rustc/|\.cargo)' > /dev/null 2> /tmp/race-err.txt
      rc=$?; n=$((n + 1))
      if [ $rc -ne 0 ]; then bad=$((bad + 1)); codes="$codes $rc"; grep -v '^hash-mismatch' /tmp/race-err.txt > "$keep/$label-$n.txt"; fi
    done
    echo "$label profile=$prof flags=[$*] runs=$n failures=$bad exit-codes=[${codes# }] llvm=$("$bin/llvm-cov" --version | sed -n 's/^ *LLVM version //p') host=$host" | tee -a "$results" ;;
  *) echo "usage: stress.sh prepare | fixprof | run LABEL SECS PROFILE [flags]" >&2; exit 2 ;;
esac
