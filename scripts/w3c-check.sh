#!/usr/bin/env bash
# Downloads the W3C XML Conformance Test Suite (xmlts20130923), checks its
# SHA-256, and compares the port with libexpat on every test document of
# its catalogue (tests/tests/w3c_diff.rs, D-059). The suite is not
# vendored; set XMLCONF_DIR to use an extracted copy instead.
set -euo pipefail
cd "$(dirname "$0")/.."

URL=https://www.w3.org/XML/Test/xmlts20130923.tar.gz
SHA256=9b61db9f5dbffa545f4b8d78422167083a8568c59bd1129f94138f936cf6fc1f
OUT=${OUT:-target/w3c}

if [ -z "${XMLCONF_DIR:-}" ]; then
  mkdir -p "$OUT"
  if [ ! -f "$OUT/xmlts.tar.gz" ]; then
    curl -sSfL --retry 3 -o "$OUT/xmlts.tar.gz.part" "$URL"
    mv "$OUT/xmlts.tar.gz.part" "$OUT/xmlts.tar.gz"
  fi
  echo "$SHA256  $OUT/xmlts.tar.gz" | sha256sum -c -
  rm -rf "$OUT/xmlconf"
  tar -xzf "$OUT/xmlts.tar.gz" -C "$OUT"
  XMLCONF_DIR=$PWD/$OUT/xmlconf
fi

XMLCONF_DIR=$XMLCONF_DIR cargo test --release -p expat-difftest --test w3c_diff -- --nocapture
