#!/usr/bin/env bash
# Compares the port with libexpat's development branch (ROADMAP point 10,
# D-062). It fetches the branch (UPSTREAM_REF, default master), lists the
# library files that changed since the vendored 2.10.0 copy, then runs
# scripts/capi-check.sh with the branch as the C side and
# scripts/runtests-check.sh with the branch's test suite. A failure here
# means libexpat changed in a way the port does not follow yet; the
# procedure in docs/RELEASING.md says what to do with it. Set UPSTREAM_SRC
# to an existing checkout instead of fetching.
set -euo pipefail
cd "$(dirname "$0")/.."

REF=${UPSTREAM_REF:-master}
OUT=${OUT:-target/upstream}
mkdir -p "$OUT"

if [ -z "${UPSTREAM_SRC:-}" ]; then
  rm -rf "$OUT/libexpat"
  git clone -q --depth 1 --branch "$REF" \
    https://github.com/libexpat/libexpat.git "$OUT/libexpat"
  UPSTREAM_SRC=$OUT/libexpat
fi
src=$UPSTREAM_SRC/expat
echo "upstream-check: libexpat $REF at $(git -C "$UPSTREAM_SRC" log -1 --format='%h %cs')"

vendor=crates/expat-oracle/vendor/expat/lib
changed=$(diff -rq "$vendor" "$src/lib" 2>/dev/null \
  | grep -v -E 'Makefile|libexpat\.(def|map)' || true)
if [ -n "$changed" ]; then
  echo "upstream-check: library files that differ from the vendored copy:"
  echo "$changed" | sed 's/^/  /'
else
  echo "upstream-check: library files equal to the vendored copy"
fi

status=0
OUT=$OUT/capi EXPAT_LIB=$src/lib scripts/capi-check.sh || status=1
OUT=$OUT/runtests EXPAT_SRC=$src EXPAT_LIB=$src/lib \
  scripts/runtests-check.sh || status=1
if [ "$status" -ne 0 ]; then
  echo "upstream-check: the port differs from libexpat $REF" >&2
fi
exit "$status"
