#!/usr/bin/env bash
# Builds q5_overread against the vendored libexpat 2.9.0 (oracle config), plain and with ASan.
set -e
here=$(cd "$(dirname "$0")" && pwd); root=$here/../../..
lib=$root/crates/expat-oracle/vendor/expat/lib; shim=$root/crates/expat-oracle/shim
src="$lib/xmlparse.c $lib/xmlrole.c $lib/xmltok.c $lib/xcs.c $lib/random_dev_urandom.c"
out=${OUT:-/tmp/q5}; mkdir -p $out
cc=${CC:-clang}
flags="-DXML_STATIC -I$shim -I$lib -g -O1"
$cc $flags $src $here/q5_overread.c -o $out/q5_plain
$cc $flags -fsanitize=address -fno-omit-frame-pointer $src $here/q5_overread.c -o $out/q5_asan
$cc $flags -fsanitize=memory -fsanitize-memory-track-origins -fno-omit-frame-pointer $src $here/q5_overread.c -o $out/q5_msan
echo built $out/q5_plain $out/q5_asan $out/q5_msan
