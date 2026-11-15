#!/usr/bin/env bash
# Builds expat-capi as a drop-in replacement for libexpat 2.10.0 on Linux
# (D-060): target/dropin/lib/libexpat.so.1.10.0 with the soname
# libexpat.so.1 (and the libexpat.so.1 / libexpat.so links), and the
# headers libexpat installs (expat.h, expat_external.h) in
# target/dropin/include. A program built against libexpat runs against it
# with LD_LIBRARY_PATH=target/dropin/lib. Extra cargo arguments (such as
# --features testing) are passed on.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=${OUT_DROPIN:-${OUT:-target/dropin}}
TARGET_DIR=${TARGET_DIR:-target/dropin-build}
lib=crates/expat-oracle/vendor/expat/lib

RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-soname,libexpat.so.1" \
  cargo build -q --release -p expat-capi --target-dir "$TARGET_DIR" "$@"

mkdir -p "$OUT/lib" "$OUT/include"
cp "$TARGET_DIR/release/libexpat_capi.so" "$OUT/lib/libexpat.so.1.10.0"
ln -sf libexpat.so.1.10.0 "$OUT/lib/libexpat.so.1"
ln -sf libexpat.so.1 "$OUT/lib/libexpat.so"
cp "$TARGET_DIR/release/libexpat_capi.a" "$OUT/lib/libexpat.a"
cp "$lib/expat.h" "$lib/expat_external.h" "$OUT/include/"
echo "build-dropin: $OUT/lib/libexpat.so.1.10.0 (soname libexpat.so.1), $OUT/include"
