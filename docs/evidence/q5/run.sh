#!/usr/bin/env bash
# Runs q5_overread (built by build.sh into $OUT, default /tmp/q5) and writes the outputs next to this script.
here=$(cd "$(dirname "$0")" && pwd); out=${OUT:-/tmp/q5}; root=$(cd $here/../../.. && pwd)
sans=${SANITIZERS:-asan msan}
# `timeout` is not installed on macOS runners.
to() { if command -v timeout >/dev/null; then timeout 30 "$@"; else "$@"; fi; }
clean() { sed -e "s#$root/docs/evidence/q5/../../../#<repo>/#g" -e "s#$root/#<repo>/#g" -e "s#$out/#<bin>/#g"; }
{
  echo "# $(uname -sm), $(${CC:-clang} --version | head -1); repo commit $(git -C $root rev-parse --short HEAD)"
  for m in parse parsebuffer; do for d in 0 1 2; do
    echo "## q5_plain $m $d"; to $out/q5_plain $m $d 2>&1; echo "exit $?"
  done; done
} 2>&1 | clean > $here/plain.txt
case " $sans " in *" asan "*) {
  echo "# $(uname -sm), $(${CC:-clang} --version | head -1); repo commit $(git -C $root rev-parse --short HEAD)"
  for m in parse parsebuffer; do for d in 0 1 2; do
    echo "## q5_asan $m $d (map with 0xBE = ASan malloc fill byte as lead of 4)"; to $out/q5_asan $m $d 2>&1; echo "exit $?"
  done; done
} 2>&1 | clean > $here/asan.txt ;; esac
case " $sans " in *" msan "*) {
  echo "# $(uname -sm), $(${CC:-clang} --version | head -1); repo commit $(git -C $root rev-parse --short HEAD)"
  for d in 0 1 2; do
    echo "## Q5_NO_BE=1 q5_msan parse $d (map: only C3 lead 2, A9 lead 4)"; Q5_NO_BE=1 to $out/q5_msan parse $d 2>&1; echo "exit $?"
  done
} 2>&1 | clean > $here/msan.txt ;; esac
true
