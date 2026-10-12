#!/usr/bin/env bash
# Runs q5_overread (built by build.sh into $OUT, default /tmp/q5) and writes the outputs next to this script.
here=$(cd "$(dirname "$0")" && pwd); out=${OUT:-/tmp/q5}; root=$(cd $here/../../.. && pwd)
clean() { sed -e "s#$root/docs/evidence/q5/../../../#<repo>/#g" -e "s#$root/#<repo>/#g" -e "s#$out/#<bin>/#g"; }
{
  echo "# $(uname -sm), $(${CC:-clang} --version | head -1); repo commit $(git -C $root rev-parse --short HEAD)"
  for m in parse parsebuffer; do for d in 0 1 2; do
    echo "## q5_plain $m $d"; timeout 30 $out/q5_plain $m $d 2>&1; echo "exit $?"
  done; done
} 2>&1 | clean > $here/plain.txt
{
  echo "# $(uname -sm), $(${CC:-clang} --version | head -1); repo commit $(git -C $root rev-parse --short HEAD)"
  for m in parse parsebuffer; do for d in 0 1 2; do
    echo "## q5_asan $m $d (map with 0xBE = ASan malloc fill byte as lead of 4)"; timeout 30 $out/q5_asan $m $d 2>&1; echo "exit $?"
  done; done
} 2>&1 | clean > $here/asan.txt
{
  echo "# $(uname -sm), $(${CC:-clang} --version | head -1); repo commit $(git -C $root rev-parse --short HEAD)"
  for d in 0 1 2; do
    echo "## Q5_NO_BE=1 q5_msan parse $d (map: only C3 lead 2, A9 lead 4)"; Q5_NO_BE=1 timeout 30 $out/q5_msan parse $d 2>&1; echo "exit $?"
  done
} 2>&1 | clean > $here/msan.txt
grep -cE "AddressSanitizer: heap-buffer-overflow" $here/asan.txt; grep -c "use-of-uninitialized-value" $here/msan.txt
