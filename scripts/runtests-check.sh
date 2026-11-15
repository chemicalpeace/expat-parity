#!/usr/bin/env bash
# Builds libexpat 2.10.0's own test suite (tests/runtests, with
# XML_TESTING, as libexpat's CMake build does) against the drop-in shared
# library (scripts/build-dropin.sh with the `testing` feature), runs it
# under every chunk size and reparse deferral setting, and compares the
# failing tests with crates/expat-capi/tests/runtests-white-box.txt: the
# tests that call libexpat internals rather than expat.h (D-060). It fails
# when another test fails, or when a listed test passes. The test sources
# are downloaded and checked against their SHA-256; set EXPAT_SRC to an
# extracted expat-2.10.0 directory instead.
set -euo pipefail
cd "$(dirname "$0")/.."

URL=https://github.com/libexpat/libexpat/releases/download/R_2_10_0/expat-2.10.0.tar.gz
SHA256=5466a341f9c692ef7a958a6881311be597f8e1ce345984cf78414f54793dea4c
OUT=${OUT:-target/runtests}
CC=${CC:-cc}
mkdir -p "$OUT/obj"

if [ -z "${EXPAT_SRC:-}" ]; then
  if [ ! -f "$OUT/expat-2.10.0.tar.gz" ]; then
    curl -sSfL --retry 3 -o "$OUT/expat.part" "$URL"
    mv "$OUT/expat.part" "$OUT/expat-2.10.0.tar.gz"
  fi
  echo "$SHA256  $OUT/expat-2.10.0.tar.gz" | sha256sum -c -
  rm -rf "$OUT/expat-2.10.0"
  tar -xzf "$OUT/expat-2.10.0.tar.gz" -C "$OUT"
  EXPAT_SRC=$OUT/expat-2.10.0
fi

OUT_DROPIN=$OUT/dropin TARGET_DIR=target/runtests-build \
  scripts/build-dropin.sh --features testing
inc=$OUT/dropin/include
libdir=$PWD/$OUT/dropin/lib
vendor=${EXPAT_LIB:-crates/expat-oracle/vendor/expat/lib}

for f in acc_tests alloc_tests basic_tests chardata common dummy handlers \
  hash_tests entities_tests memcheck minicheck misc_tests ns_tests \
  nsalloc_tests props_tests runtests structdata xcs_tests; do
  # The tests also include libexpat's internal headers (declarations of
  # the internals listed as white-box), taken from the vendored copy, and
  # expat_config.h, from the release or else the oracle's (a git checkout
  # has none).
  "$CC" -c -O1 -w -DXML_TESTING "-I$inc" "-I$vendor" "-I$EXPAT_SRC" \
    -Icrates/expat-oracle/shim "$EXPAT_SRC/tests/$f.c" -o "$OUT/obj/$f.o"
done
# The xcs string helpers the tests use are plain C, not part of the parser.
"$CC" -c -O1 -w "-I$vendor" "$vendor/xcs.c" -o "$OUT/obj/xcs.o"
"$CC" -c -O1 -w "-I$inc" "-I$EXPAT_SRC/tests" \
  crates/expat-capi/tests/c/runtests_whitebox.c -o "$OUT/obj/whitebox.o"
"$CC" "$OUT"/obj/*.o "-L$libdir" -lexpat "-Wl,-rpath,$libdir" -lm \
  -o "$OUT/runtests"
# The program must run against the drop-in library, not a system libexpat.
ldd "$OUT/runtests" | grep -q "$libdir/libexpat.so.1"

set +e
"$OUT/runtests" -v > "$OUT/runtests.txt" 2> "$OUT/runtests-stderr.txt"
status=$?
set -e
tail -n 1 "$OUT/runtests.txt"
grep '^FAIL' "$OUT/runtests.txt" | sed 's/^FAIL \[[^]]*\]: //; s/ (.*//' \
  | sort -u > "$OUT/failing.txt"
grep -v '^#' crates/expat-capi/tests/runtests-white-box.txt | sed 's/ .*//' \
  | grep . | sort -u > "$OUT/expected.txt"
if ! diff -u "$OUT/expected.txt" "$OUT/failing.txt"; then
  echo "runtests-check: failing tests differ from the white-box list" >&2
  exit 1
fi
passed=$(grep -c '^PASS' "$OUT/runtests.txt" || true)
echo "runtests-check: $passed passing runs; failing only the $(wc -l < "$OUT/expected.txt") white-box tests (runtests exit $status)"
