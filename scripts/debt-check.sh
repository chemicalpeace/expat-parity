#!/usr/bin/env bash
# Zero-tech-debt gate. Fails (exit 1) on any violation. See docs/DECISIONS.md D-011.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
report() { echo "debt-check: $1"; fail=1; }

# Rust sources we own (vendored C and build output excluded).
mapfile -t RS < <(git ls-files --cached --others --exclude-standard -- '*.rs' | grep -v '^crates/expat-oracle/vendor/' || true)
# Library code: no panicking helpers. Unit tests live in files named tests.rs.
mapfile -t LIB < <(printf '%s\n' "${RS[@]}" | grep -E '^crates/expat(-capi)?/src/' | grep -v '/tests\.rs$' || true)
# Code that must never contain `unsafe`.
mapfile -t SAFE < <(printf '%s\n' "${RS[@]}" | grep -vE '^crates/expat-(oracle|capi)/' || true)
# All text we own, for marker words.
mapfile -t TXT < <(git ls-files --cached --others --exclude-standard | grep -vE '^crates/expat-oracle/vendor/|^docs/|^scripts/debt-check\.sh$|^CONTRIBUTING\.md$|^fuzz/corpus/|^tests/vectors/|\.lock$' || true)

check() { # pattern, description, files...
  local pat=$1 desc=$2; shift 2
  [ $# -eq 0 ] && return 0
  local hits
  hits=$(grep -nE -- "$pat" "$@" 2>/dev/null || true)
  if [ -n "$hits" ]; then report "$desc"; echo "$hits" | sed 's/^/    /'; fi
}

check '\b(todo|unimplemented)!' 'todo!/unimplemented! is forbidden' "${RS[@]}"
check '\b(TODO|FIXME|XXX|HACK)\b' 'debt marker words are forbidden' "${TXT[@]}"
check '#\[ignore' 'ignored tests are forbidden' "${RS[@]}"
check '\.unwrap\(\)|\.expect\(|\bpanic!|\bunreachable!|\bunwrap_unchecked' \
  'panicking helpers are forbidden in library code' "${LIB[@]}"
check '\bunsafe\b' 'unsafe is only allowed in expat-oracle and expat-capi' "${SAFE[@]}"
# Commented-out code heuristic (doc comments //! and /// are fine).
check '^\s*//[^/!].*(;\s*$|\{\s*$|^\s*//\s*(let|fn|use|pub|impl|match|struct|enum|return)\b)' \
  'commented-out code is forbidden' "${RS[@]}"

# Every #[allow(..)] / #![allow(..)] needs a `// reason:` on the same or previous line.
for f in "${RS[@]}"; do
  awk -v f="$f" '
    /#!?\[allow\(/ { if ($0 !~ /reason:/ && prev !~ /reason:/) { print "    " f ":" NR ": " $0; bad=1 } }
    { prev=$0 }
    END { exit bad }' "$f" || report "#[allow] without '// reason:' justification"
done

if [ $fail -ne 0 ]; then echo "debt-check: FAILED"; exit 1; fi
echo "debt-check: OK (${#RS[@]} Rust files, ${#LIB[@]} library files)"
