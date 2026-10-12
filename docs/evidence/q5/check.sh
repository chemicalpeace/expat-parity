#!/usr/bin/env bash
# Checks the outputs of run.sh against the documented Q-5 results
# (docs/security/Q-5-libexpat-2.9.0-unknown_toUtf8-overread.md): exits 1 if a
# sanitizer report is missing or lands elsewhere. SANITIZERS as in build.sh.
here=$(cd "$(dirname "$0")" && pwd); sans=${SANITIZERS:-asan msan}; fail=0
expect() { # file pattern count what
  local n; n=$(grep -cE "$2" "$here/$1"); echo "$4: $n (expected $3)"; [ "$n" -eq "$3" ] || fail=1; }
# Plain build: with the rest of XML_GetBuffer's block set by the program
# (parsebuffer), docs 0 and 1 make the converter read past the data and doc 2
# does not. (In parse mode the bytes after the data are whatever the heap
# holds, so those counts are not checked.)
expect plain.txt 'doc [01] .*mode parsebuffer: .*calls reading past the document [1-9]' 2 "plain parsebuffer: docs 0 and 1 read past the data"
expect plain.txt 'doc 2 .*calls reading past the document 0,' 2 "plain: doc 2 (data after the declaration) never reads past it"
case " $sans " in *" asan "*)
  expect asan.txt '^SUMMARY: AddressSanitizer: heap-buffer-overflow .*xmltok\.c:1283:[0-9]+ in unknown_toUtf8' 4 "ASan heap-buffer-overflow in unknown_toUtf8 (xmltok.c:1283)"
  expect asan.txt 'in XML_GetBuffer .*xmlparse\.c:2603' 4 "ASan: overflowed block allocated by XML_GetBuffer (xmlparse.c:2603)" ;; esac
case " $sans " in *" msan "*)
  expect msan.txt '^SUMMARY: MemorySanitizer: use-of-uninitialized-value .*xmltok\.c:1284:[0-9]+ in unknown_toUtf8' 2 "MSan use-of-uninitialized-value in unknown_toUtf8 (xmltok.c:1284)" ;; esac
exit $fail
