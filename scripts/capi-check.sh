#!/usr/bin/env bash
# Builds crates/expat-capi/tests/c/trace.c once against the vendored
# libexpat 2.10.0 and once against expat-capi, runs both on the upstream
# test vectors and on tests/vectors/handwritten-rich.xml, and fails if the
# outputs differ (module 13, D-047).
set -euo pipefail
cd "$(dirname "$0")/.."

CC=${CC:-cc}
OUT=${OUT:-target/capi-check}
mkdir -p "$OUT/docs"
lib=crates/expat-oracle/vendor/expat/lib

cargo build -q --release -p expat-capi
"$CC" -O1 -w -DXML_STATIC -Icrates/expat-oracle/shim "-I$lib" \
  crates/expat-capi/tests/c/trace.c "$lib/xmlparse.c" "$lib/xmlrole.c" \
  "$lib/xmltok.c" "$lib/xcs.c" "$lib/random_dev_urandom.c" -o "$OUT/trace-c"
"$CC" -O1 -w "-I$lib" crates/expat-capi/tests/c/trace.c \
  target/release/libexpat_capi.a -lpthread -ldl -lm -o "$OUT/trace-rs"

python3 - "$OUT/docs" <<'PY'
import json, sys, pathlib
out = pathlib.Path(sys.argv[1])
for i, line in enumerate(open("tests/vectors/upstream.jsonl")):
    (out / f"{i:04}.xml").write_bytes(bytes.fromhex(json.loads(line)["doc_hex"]))
PY
cp tests/vectors/handwritten-rich.xml "$OUT/docs/rich.xml"

"$OUT/trace-c" "$OUT"/docs/*.xml > "$OUT/c.txt"
"$OUT/trace-rs" "$OUT"/docs/*.xml > "$OUT/rs.txt"
if ! cmp -s "$OUT/c.txt" "$OUT/rs.txt"; then
  diff "$OUT/c.txt" "$OUT/rs.txt" | head -40
  echo "capi-check: outputs differ" >&2
  exit 1
fi
echo "capi-check: $(grep -c '^doc ' "$OUT/c.txt") documents, $(grep -c '^ config ' "$OUT/c.txt") runs, outputs equal"
