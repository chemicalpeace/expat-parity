#!/usr/bin/env bash
# Throughput of the port, of C libexpat 2.10.0 (clang/gcc -O2) and of C
# libexpat built with Fil-C. See benches/filc/README.md.
#
#   benches/filc/run.sh build               build every harness into $BIN
#   benches/filc/run.sh corpus              generate the corpus into $CORPUS
#   benches/filc/run.sh run [FILE...]       run the matrix, append to $OUT
#
# Environment: FILC (Fil-C distribution dir, required for `build`), BIN
# (default /tmp/expat-bench/bin), CORPUS (default /tmp/expat-bench/corpus),
# OUT (default /tmp/expat-bench/results.txt), RUNS (default 7), CPU (core to
# pin to with taskset, default 1), IMPLS, MODES, CHUNKS (subsets of the
# default matrix, space-separated).
set -euo pipefail
cd "$(dirname "$0")/../.."

BIN=${BIN:-/tmp/expat-bench/bin}
CORPUS=${CORPUS:-/tmp/expat-bench/corpus}
OUT=${OUT:-/tmp/expat-bench/results.txt}
RUNS=${RUNS:-7}
CPU=${CPU:-1}
read -r -a IMPLS <<<"${IMPLS:-rust-lto rust-release c-clang c-gcc c-filc}"
read -r -a MODES <<<"${MODES:-none count}"
read -r -a CHUNKS <<<"${CHUNKS:-0 65536}"

build() {
  mkdir -p "$BIN"
  local lib=crates/expat-oracle/vendor/expat/lib
  local src=(benches/filc/xmlbench.c "$lib/xmlparse.c" "$lib/xmlrole.c"
    "$lib/xmltok.c" "$lib/xcs.c" "$lib/random_dev_urandom.c")
  # The oracle's configuration header: XML_DTD, XML_GE=1, XML_NS,
  # XML_CONTEXT_BYTES=1024, UTF-8 XML_Char (the port's configuration).
  local flags=(-O2 -DNDEBUG -DXML_STATIC -Icrates/expat-oracle/shim "-I$lib" -w -g)
  clang "${flags[@]}" "${src[@]}" -o "$BIN/c-clang"
  gcc "${flags[@]}" "${src[@]}" -o "$BIN/c-gcc"
  "${FILC:?set FILC to the Fil-C distribution directory}/build/bin/clang" \
    "${flags[@]}" "${src[@]}" -o "$BIN/c-filc"
  local t=${CARGO_TARGET_DIR:-target}
  cargo build --release -p expat-bench --bin xmlbench
  cp "$t/release/xmlbench" "$BIN/rust-release"
  CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_TARGET_DIR="$t/lto" cargo build --release -p expat-bench --bin xmlbench
  cp "$t/lto/release/xmlbench" "$BIN/rust-lto"
}

run() {
  mkdir -p "$(dirname "$OUT")"
  local files=("$@")
  [ ${#files[@]} -eq 0 ] && files=("$CORPUS"/*)
  local pin=()
  command -v taskset >/dev/null && pin=(taskset -c "$CPU")
  for f in "${files[@]}"; do
    local ns=0
    case "$(basename "$f")" in ns-*) ns=1 ;; esac
    for mode in "${MODES[@]}"; do
      for chunk in "${CHUNKS[@]}"; do
        for impl in "${IMPLS[@]}"; do
          local line
          line=$("${pin[@]}" "$BIN/$impl" "$f" "$mode" "$chunk" "$ns" "$RUNS")
          echo "file=$(basename "$f") mode=$mode chunk=$chunk ns=$ns bin=$impl $line" | tee -a "$OUT"
        done
      done
    done
  done
}

case "${1:-}" in
  build) build ;;
  corpus) python3 benches/filc/gen_corpus.py "$CORPUS" ;;
  run) shift; run "$@" ;;
  *) echo "usage: $0 build|corpus|run [FILE...]" >&2; exit 2 ;;
esac
