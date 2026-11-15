#!/usr/bin/env bash
# Measures large benign documents with their DTDs against libexpat's
# default amplification limits (ROADMAP point 3, D-063): the PostgreSQL 17
# manual (DocBook XML 4.5, one book assembled from external entities) and
# the DITA Open Toolkit 4.3.4 user guide (DITA 1.3 topics and maps).
# benches/docs/measure.c is built against the vendored libexpat and
# against expat-capi; the script fails if the two outputs differ, and
# writes them to $OUT/results.txt. Needs the DocBook XML 4.5 DTD
# (Debian/Ubuntu package docbook-xml) and network access for the rest.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=${OUT:-target/doc-limits}
CC=${CC:-cc}
DOCBOOK=${DOCBOOK:-/usr/share/xml/docbook/schema/dtd/4.5}
PG_TAG=REL_17_12
DITA=dita-ot-4.3.4
DITA_URL=https://github.com/dita-ot/dita-ot/releases/download/4.3.4/$DITA.zip
DITA_SHA256=011c65bbe55b917f572e8d8e81a5518504cff0706b436f1c8a6565a1d2d98f08
mkdir -p "$OUT"

[ -f "$DOCBOOK/docbookx.dtd" ] || {
  echo "doc-limits: DocBook XML 4.5 DTD not found in $DOCBOOK (package docbook-xml)" >&2
  exit 1
}

if [ ! -d "$OUT/pg" ]; then
  git clone -q --depth 1 --filter=blob:none --sparse --branch "$PG_TAG" \
    https://github.com/postgres/postgres.git "$OUT/pg"
  git -C "$OUT/pg" sparse-checkout set doc/src/sgml
fi
sgml=$OUT/pg/doc/src/sgml
# Files the PostgreSQL build generates are left empty here.
printf '<!ENTITY version "17">\n<!ENTITY majorversion "17">\n' > "$sgml/version.sgml"
grep -ho 'SYSTEM "[^"]*"' "$sgml/filelist.sgml" "$sgml/postgres.sgml" \
  | sed 's/SYSTEM "//; s/"$//' | while read -r f; do
  [ -e "$sgml/$f" ] || : > "$sgml/$f"
done

if [ ! -d "$OUT/$DITA/plugins/org.oasis-open.xdita.v0_3_0" ]; then
  [ -f "$OUT/$DITA.zip" ] || curl -sSfL --retry 3 -o "$OUT/$DITA.zip" "$DITA_URL"
  echo "$DITA_SHA256  $OUT/$DITA.zip" | sha256sum -c --quiet -
  unzip -q -o "$OUT/$DITA.zip" "$DITA/docsrc/*" "$DITA/catalog-dita.xml" \
    "$DITA/plugins/org.dita.base/*" "$DITA/plugins/org.oasis-open.dita*/*" "$DITA/plugins/org.oasis-open.xdita*/*" \
    "$DITA/plugins/org.dita.specialization*/*" -d "$OUT"
fi

python3 benches/docs/catalog.py "$OUT/$DITA/catalog-dita.xml" \
  "$DOCBOOK/catalog.xml" > "$OUT/catalog.tsv"

lib=crates/expat-oracle/vendor/expat/lib
cargo build -q --release -p expat-capi
"$CC" -O2 -w -DXML_GE=1 -DXML_STATIC -Icrates/expat-oracle/shim "-I$lib" \
  benches/docs/measure.c "$lib/xmlparse.c" "$lib/xmlrole.c" "$lib/xmltok.c" \
  "$lib/xcs.c" "$lib/random_dev_urandom.c" -lm -o "$OUT/measure-c"
"$CC" -O2 -w -DXML_GE=1 "-I$lib" benches/docs/measure.c \
  target/release/libexpat_capi.a -lpthread -ldl -lm -o "$OUT/measure-rs"

# Full measurement for the two books and the largest topics; every other
# DITA document is parsed with the defaults only.
cd "$OUT"
full=(pg/doc/src/sgml/postgres.sgml "$DITA/docsrc/userguide-book.ditamap"
  "$DITA/docsrc/userguide.ditamap")
mapfile -t largest < <(find "$DITA/docsrc" -name '*.dita' -printf '%s %p\n' \
  | sort -rn | head -n 5 | cut -d' ' -f2-)
mapfile -t rest < <(find "$DITA/docsrc" \( -name '*.dita' -o -name '*.ditamap' \) \
  | sort | grep -v -x -F -f <(printf '%s\n' "${full[@]}" "${largest[@]}"))
args=(catalog.tsv "${full[@]}" "${largest[@]}" -q "${rest[@]}")
./measure-c "${args[@]}" > c.txt 2> c-stderr.txt
./measure-rs "${args[@]}" > rs.txt 2> rs-stderr.txt
if ! cmp -s c.txt rs.txt || ! cmp -s c-stderr.txt rs-stderr.txt; then
  diff c.txt rs.txt | head -20
  diff c-stderr.txt rs-stderr.txt | head -20
  echo "doc-limits: outputs differ" >&2
  exit 1
fi
cp c.txt results.txt
echo "doc-limits: $(wc -l < c.txt) documents, outputs equal; refused with the defaults: $(grep -c ' error=' c.txt || true)"
