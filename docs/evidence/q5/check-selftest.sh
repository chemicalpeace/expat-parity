#!/usr/bin/env bash
# Self-test of check.sh: it must ACCEPT the recorded sanitizer outputs of both
# toolchains (Linux clang: `<path>/xmltok.c:1283:38`; Apple clang:
# `xmltok.c:1283`) and REJECT outputs that lack the documented reports or show
# a different crash. Every negative case is derived from a real recorded
# output by one edit, so a pass here means check.sh tells exactly that edit
# apart. Uses only the checked-in files (run it before run.sh overwrites them).
#
#   asan.txt                                 Linux aarch64, Debian clang 19.1.7
#   fixtures/asan-macos-apple-clang-21.txt   macOS arm64, Apple clang 21.0.0
#                                            (CI run 38248323590, artifact
#                                            q5-sanitizers-macOS)
#   msan.txt, plain.txt                      Linux aarch64, Debian clang 19.1.7
set -u
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
pass=0; bad=0

# case NAME EXPECT(0|1) SANITIZERS ASAN_FILE MSAN_FILE
case_() {
  local name=$1 want=$2 sans=$3 asan=$4 msan=$5 got
  rm -rf "$tmp/c"; mkdir "$tmp/c"
  cp "$here/check.sh" "$here/plain.txt" "$tmp/c/"
  cp "$asan" "$tmp/c/asan.txt"; cp "$msan" "$tmp/c/msan.txt"
  SANITIZERS="$sans" bash "$tmp/c/check.sh" > "$tmp/c/out" 2>&1; got=$?
  if { [ "$want" -eq 0 ] && [ "$got" -eq 0 ]; } || { [ "$want" -ne 0 ] && [ "$got" -ne 0 ]; }; then
    pass=$((pass + 1)); echo "ok    $name (check.sh exit $got)"
  else
    bad=$((bad + 1)); echo "FAIL  $name (check.sh exit $got, wanted $([ "$want" -eq 0 ] && echo 0 || echo non-zero))"
    sed 's/^/      /' "$tmp/c/out"
  fi
}

lin=$here/asan.txt
mac=$here/fixtures/asan-macos-apple-clang-21.txt
msan=$here/msan.txt
m() { sed "$1" "$2" > "$tmp/$3"; echo "$tmp/$3"; }

# Accepted: the real outputs.
case_ "Linux clang ASan+MSan output"            0 "asan msan" "$lin" "$msan"
case_ "Apple clang ASan output (no column)"     0 "asan"      "$mac" "$msan"

for src in lin mac; do
  f=$(eval echo \$$src)
  # The output lacks the reports: program ran clean (no ASan report at all).
  awk '/^=+$/{skip=1} /^exit /{skip=0} !skip' "$f" > "$tmp/$src-noreport"
  case_ "$src: no ASan report at all"            1 "asan" "$tmp/$src-noreport" "$msan"
  # One of the four reports missing.
  awk '/^SUMMARY: AddressSanitizer/{n++; if (n==4) next} {print}' "$f" > "$tmp/$src-three"
  case_ "$src: only 3 of the 4 reports"         1 "asan" "$tmp/$src-three" "$msan"
  # A different crash at the same place: SEGV instead of heap-buffer-overflow.
  case_ "$src: SEGV instead of heap-buffer-overflow" 1 "asan" \
    "$(m 's/heap-buffer-overflow/SEGV/g' "$f" $src-segv)" "$msan"
  # A different crash kind: heap-use-after-free.
  case_ "$src: heap-use-after-free instead"     1 "asan" \
    "$(m 's/heap-buffer-overflow/heap-use-after-free/g' "$f" $src-uaf)" "$msan"
  # Same kind, other line / other file / other function.
  case_ "$src: overflow at xmltok.c:1284"       1 "asan" "$(m 's/xmltok\.c:1283/xmltok.c:1284/g' "$f" $src-l1284)" "$msan"
  case_ "$src: overflow at xmltok.c:12830"      1 "asan" "$(m 's/xmltok\.c:1283/xmltok.c:12830/g' "$f" $src-l12830)" "$msan"
  case_ "$src: overflow in xmlrole.c:1283"      1 "asan" "$(m 's/xmltok\.c:1283/xmlrole.c:1283/g' "$f" $src-role)" "$msan"
  case_ "$src: overflow in normal_toUtf8"       1 "asan" "$(m 's/1283\(:[0-9]*\)* in unknown_toUtf8/1283\1 in normal_toUtf8/' "$f" $src-func)" "$msan"
  # Overflowed block allocated somewhere else.
  case_ "$src: block allocated at xmlparse.c:2604" 1 "asan" "$(m 's/xmlparse\.c:2603/xmlparse.c:2604/g' "$f" $src-a2604)" "$msan"
  case_ "$src: block allocated by poolGrow"     1 "asan" "$(m 's/in XML_GetBuffer /in poolGrow /' "$f" $src-pool)" "$msan"
done

# MSan (Linux only).
awk '/^SUMMARY: MemorySanitizer/{n++; if (n==2) next} {print}' "$msan" > "$tmp/msan-one"
case_ "msan: only 1 of the 2 reports"            1 "asan msan" "$lin" "$tmp/msan-one"
case_ "msan: report at xmltok.c:1283"            1 "asan msan" "$lin" "$(m 's/xmltok\.c:1284/xmltok.c:1283/g' "$msan" msan-l)"

# Plain build: no read past the document.
rm -rf "$tmp/c"; mkdir "$tmp/c"; cp "$here/check.sh" "$tmp/c/"
sed 's/calls reading past the document [1-9][0-9]*/calls reading past the document 0/' "$here/plain.txt" > "$tmp/c/plain.txt"
cp "$lin" "$tmp/c/asan.txt"; cp "$msan" "$tmp/c/msan.txt"
if SANITIZERS="asan msan" bash "$tmp/c/check.sh" > /dev/null 2>&1; then
  bad=$((bad + 1)); echo "FAIL  plain: no read past the data (check.sh accepted it)"
else pass=$((pass + 1)); echo "ok    plain: no read past the data (check.sh rejects)"; fi

echo "check-selftest: $pass ok, $bad failed"
[ "$bad" -eq 0 ]
