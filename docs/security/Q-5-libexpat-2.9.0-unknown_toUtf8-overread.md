# Q-5: out-of-bounds read in libexpat 2.9.0 `unknown_toUtf8` (via `XmlParseXmlDecl`)

Status: found while porting (docs/DIVERGENCES.md Q-5, D-035). **Not
reported upstream yet** (see "Responsible-disclosure status"). Reproduced only as described under "What is verified". This
document is a reproducible record, not a vulnerability advisory: no claim
is made about exploitability or about versions other than the one tested.

## Summary

When an application installs an unknown-encoding handler
(`XML_SetUnknownEncodingHandler`) whose map declares multi-byte lead bytes
and gives a `convert` function, and the document's XML declaration ends
where a pseudo-attribute is being scanned, libexpat 2.9.0 reads bytes past
the end of the data it was given: `unknown_toUtf8` advances its input
pointer by the length the map announces for a lead byte without comparing
against the end of the input, and its loop only stops when the pointer is
*equal* to the end. AddressSanitizer reports a heap-buffer-overflow READ in
`unknown_toUtf8` past the parser's input buffer; MemorySanitizer reports a
use of uninitialised heap memory at the same place.

## Version and build

- libexpat **2.9.0**, tag `R_2_9_0`, commit
  `a76b1174c1ea52abb639a4c69d52b630a8a106a2`, vendored unmodified in
  `crates/expat-oracle/vendor/expat` (checked by `scripts/verify-vendor.sh`).
- Configuration: `crates/expat-oracle/shim/expat_config.h` (`XML_DTD`,
  `XML_GE=1`, `XML_NS`, `XML_CONTEXT_BYTES=1024`, UTF-8 `XML_Char`).
- Compiler for the recorded outputs: Debian clang 19.1.7, Linux aarch64,
  `-g -O1` (`docs/evidence/q5/build.sh`).

## Minimal input

Program: `docs/evidence/q5/q5_overread.c` (public API only).

- Parser: `XML_ParserCreate("x-q5")`; the protocol encoding is unknown, so
  `XML_SetUnknownEncodingHandler` is consulted before the XML declaration is
  parsed.
- Encoding map (`XML_Encoding.map`): bytes 0x00-0x7F map to themselves;
  `map[0xC3] = -2` (lead of a 2-byte sequence); `map[0xA9] = -4` (lead of
  a 4-byte sequence); `map[0xBE] = -4` (ASan build only; 0xBE is ASan's
  malloc fill byte, so freshly allocated heap continues the walk; the MSan
  run sets `Q5_NO_BE=1` and leaves 0xBE malformed); every other byte `-1`.
  `convert` returns U+0049 for `C3 A9` and -1 otherwise, and logs where it
  is called.
- Documents (no trailing data):
  - doc 0: `<?xml \xC3\xA9?>` (10 bytes)
  - doc 1: `<?xml no\xC3\xA9 ?>` (13 bytes)
  - doc 2 (control): `<?xml \xC3\xA9?><a/>` (14 bytes; data follows the
    declaration)
- Calls: mode `parse` = `XML_Parse(p, doc, len, 1)`; mode `parsebuffer` =
  `XML_GetBuffer(p, 4096)`, rest of the buffer filled with 0xBE, document
  copied in, `XML_ParseBuffer(p, len, 1)`.

doc 0 and doc 1 are the two inputs the port's random test
`init_diff::xml_decl_in_every_builtin_encoding` found (seeds
`0x52f43d23123766f6` and `0xfe8d9b29cbd0ff22`), reduced to a fixed map.

## Reproduction steps

Needs a checkout of this repository and clang with the ASan runtime (and the
MSan runtime on Linux; Apple clang has none).

```sh
docs/evidence/q5/build.sh     # OUT=/tmp/q5 by default; SANITIZERS="asan msan" by default
docs/evidence/q5/run.sh       # writes docs/evidence/q5/{plain,asan,msan}.txt
docs/evidence/q5/check.sh     # exits 1 unless the reports below are all present
docs/evidence/q5/check-selftest.sh  # check.sh accepts the recorded outputs, rejects 23 edited ones
```

`build.sh` compiles `xmlparse.c xmlrole.c xmltok.c` (+ the oracle's
`xcs.c random_dev_urandom.c`) with the program three times: plain,
`-fsanitize=address -fno-omit-frame-pointer`, and
`-fsanitize=memory -fsanitize-memory-track-origins -fno-omit-frame-pointer`.
CI job `q5-sanitizers` runs the same three scripts on ubuntu-latest (ASan
and MSan) and macos-latest (ASan; Apple clang has no MSan) and uploads the
outputs as artifacts `q5-sanitizers-<OS>`.

## Results (docs/evidence/q5, recorded at repo commit 020a0618; re-run on main a990a8c in `rerun-main-a990a8c/`, same results)

**Plain build** (`plain.txt`): the converter is called on bytes after the
document. `parse` doc 0: 5 calls, 1 reading past the document, highest
offset read 10 (document length 10). `parsebuffer` doc 0: 1,027 calls,
1,023 past the document, highest offset 4,098; doc 1: 1,025 / 1,021 /
4,096. Doc 2 (data after the declaration): no read past the document.
Every run ends with `XML_ERROR_XML_DECL` (30), the expected error.

**ASan** (`asan.txt`), 4 reports, `parse` and `parsebuffer` x doc 0, 1
(doc 2: none):

```
==22988==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x51d000000883 ...
READ of size 1 at 0x51d000000883 thread T0
    #0 in unknown_toUtf8 .../lib/xmltok.c:1283:38
    #1 in toAscii .../lib/xmltok.c:918:3
    #2 in parsePseudoAttribute .../lib/xmltok.c:963:9
    #3 in doParseXmlDecl .../lib/xmltok.c:1071:9
    #4 in XmlParseXmlDecl .../lib/xmltok_ns.c:120:10
    #5 in processXmlDecl .../lib/xmlparse.c:5143:9
    #6 in doProlog .../lib/xmlparse.c:5598:31
    #7 in prologProcessor .../lib/xmlparse.c:5478:10
    #8 in prologInitProcessor .../lib/xmlparse.c:5271:10
    #9 in callProcessor .../lib/xmlparse.c:1247:11
    #10 in XML_ParseBuffer .../lib/xmlparse.c:2469:25
0x51d000000883 is located 3 bytes after 2048-byte region [0x51d000000080,0x51d000000880)
allocated by thread T0 here:
    #0 in malloc
    #1 in XML_GetBuffer .../lib/xmlparse.c:2603:28
    #2 in XML_Parse .../lib/xmlparse.c:2403:16
SUMMARY: AddressSanitizer: heap-buffer-overflow .../lib/xmltok.c:1283:38 in unknown_toUtf8
```

(`XML_Parse` copies the data into a 2,048-byte block from `XML_GetBuffer`;
in `parsebuffer` mode the block is the 4,096-byte one the application
requested. Full traces with shadow bytes in `asan.txt`.)

**MSan** (`msan.txt`, map without 0xBE), 2 reports (`parse` doc 0, 1):

```
==23014==WARNING: MemorySanitizer: use-of-uninitialized-value
    #0 in unknown_toUtf8 .../lib/xmltok.c:1284:9
    #1 in toAscii .../lib/xmltok.c:918:3
    #2 in parsePseudoAttribute .../lib/xmltok.c:963:9
    ... (as above) ...
  Uninitialized value was created by a heap allocation
    #0 in malloc
    #1 in XML_GetBuffer .../lib/xmlparse.c:2603:28
    #2 in XML_Parse .../lib/xmlparse.c:2403:16
SUMMARY: MemorySanitizer: use-of-uninitialized-value .../lib/xmltok.c:1284:9 in unknown_toUtf8
```

## Source locations (libexpat 2.9.0)

- `lib/xmltok.c` 1273-1301 `unknown_toUtf8`:
  - 1281 `if (*fromP == fromLim) return XML_CONVERT_COMPLETED;` — the only
    end test, an equality;
  - 1283 `utf8 = uenc->utf8[(unsigned char)**fromP];` — the out-of-bounds
    read ASan reports;
  - 1284 `n = *utf8++;` — where MSan reports the uninitialised byte's use;
  - 1286 `uenc->convert(uenc->userData, *fromP)` — the application's
    converter is handed a pointer whose announced sequence extends past
    the data;
  - 1291-1292 `*fromP += (type[**fromP] - (BT_LEAD2 - 2));` — advances by
    the announced sequence length (here 4) without checking `fromLim`, so
    `*fromP` can jump over `fromLim` and the equality test never stops it.
- `lib/xmltok.c` 915-923 `toAscii` (918: `XmlUtf8Convert(enc, &ptr, end, &p, p + 1)`)
  and 963 in `parsePseudoAttribute` (one of its ten calls in
  `parsePseudoAttribute` / `doParseXmlDecl`), which calls it byte by byte
  (`minBytesPerChar` = 1 for an unknown encoding), so the conversion can
  start on a byte inside a sequence (here 0xA9 after 0xC3), which the map
  declares a 4-byte lead.
- `lib/xmlparse.c` 2603 `parser->m_mem.malloc_fcn(bufferSize)` in
  `XML_GetBuffer`: the input buffer the read leaves.

## Affected API path (observed)

`XML_Parse` or `XML_GetBuffer` + `XML_ParseBuffer` -> `callProcessor` ->
`prologInitProcessor` -> `prologProcessor` -> `doProlog` ->
`processXmlDecl` -> `XmlParseXmlDecl` -> `doParseXmlDecl` ->
`parsePseudoAttribute` -> `toAscii` -> `XmlUtf8Convert` =
`unknown_toUtf8`, with an unknown encoding installed by the application's
unknown-encoding handler.

## What is verified

- The over-read, on libexpat 2.9.0 as vendored, with the program and map
  above, on Linux aarch64 with clang 19.1.7 (ASan and MSan), for the two
  inputs whose XML declaration ends the data, through both `XML_Parse` and
  `XML_ParseBuffer`; not with data after the declaration (doc 2).
- Re-run 2026-10-12 on main a990a8c with the same results
  (`docs/evidence/q5/rerun-main-a990a8c/`); CI job `q5-sanitizers` repeats
  it on every run (ubuntu ASan + MSan, macOS ASan; see the PR for run IDs).
- Also reproduced in CI run 38246973844 (PR #17, merge commit 25922ca0,
  artifacts `q5-sanitizers-Linux` / `q5-sanitizers-macOS`):
  - Linux x86_64, Ubuntu clang 18.1.3: ASan 4 heap-buffer-overflow reports
    at `xmltok.c:1283` in `unknown_toUtf8`, each on a block allocated at
    `xmlparse.c:2603` (`XML_GetBuffer`); MSan 2 use-of-uninitialized-value
    reports at `xmltok.c:1284`.
  - macOS (Darwin arm64), Apple clang version 21.0.0 (clang-2100.1.1.101):
    ASan, the same 4 reports at `xmltok.c:1283` / allocations at
    `xmlparse.c:2603`. Apple clang symbolizes without a column
    (`xmltok.c:1283` rather than `<path>/xmltok.c:1283:38`); `check.sh`
    accepts both forms and still requires file, line and function exactly.
    MSan is not available with Apple clang, so not run on macOS.
- Re-run in CI run 38248323590 (PR #17 head 98d87c5e): the same counts on
  Linux x86_64 (ASan + MSan) and macOS arm64 (ASan); `check.sh` passed on
  both.
- Why the macOS job of run 38246973844 failed: not a missing report. Its
  `asan.txt` (artifact `q5-sanitizers-macOS`) has all four reports, e.g.
  `SUMMARY: AddressSanitizer: heap-buffer-overflow xmltok.c:1283 in unknown_toUtf8`
  and `#1 0x00010421a0fc in XML_GetBuffer xmlparse.c:2603`; the check
  pattern then required a column (`xmltok\.c:1283:[0-9]+`), which Linux clang
  prints (`<repo>/crates/expat-oracle/vendor/expat/lib/xmltok.c:1283:38`) and
  Apple clang does not, so it counted 0 SUMMARY lines while the
  `XML_GetBuffer` pattern (no column required) counted 4.
  `check-selftest.sh` (run in CI before the build) shows `check.sh` accepts
  both recorded outputs and rejects 23 edited ones: no report, 3 of 4
  reports, SEGV or heap-use-after-free instead of heap-buffer-overflow,
  other line/file/function, other allocation site, missing MSan report,
  no plain-build over-read.
- The text of `unknown_toUtf8` is identical on libexpat `master` as fetched
  on 2026-10-12 (latest commit touching `xmltok.c`: c002e805, 2026-10-04).
  That is a source comparison only; master was **not** built or tested.

### Platforms

| Platform / compiler | Plain build | ASan | MSan | Where |
|---|---|---|---|---|
| Linux aarch64, Debian clang 19.1.7 | reads past the data | 4 reports | 2 reports | `docs/evidence/q5/*.txt`, `rerun-main-a990a8c/` (local) |
| Linux x86_64, Ubuntu clang 18.1.3 | reads past the data | 4 reports | 2 reports | CI 38246973844, 38248323590 (`q5-sanitizers-Linux`) |
| macOS arm64, Apple clang 21.0.0 (clang-2100.1.1.101) | reads past the data | 4 reports | not available | CI 38246973844, 38248323590 (`q5-sanitizers-macOS`) |
| Windows (MSVC or clang-cl) | **not tested** | **not tested** | **not tested** | no sanitizer build is set up |
| GCC, other clang versions, 32-bit, big-endian | **not tested** | | | |

Version: only libexpat 2.9.0 (`R_2_9_0`, a76b1174) was built and run.

## What is not verified / uncertainties (no claim made)

- Whether the bytes read can change what the parser reports, leak to the
  application, or crash a non-sanitised build. In the plain build every
  run ended with the expected `XML_ERROR_XML_DECL`.
- Whether real-world unknown-encoding maps (which normally declare lead
  bytes consistently with their converter) can reach the path; the map
  here makes a trail byte (0xA9) a 4-byte lead on purpose. The random
  test's maps were random too.
- Other entry points that call `toAscii` with an unknown encoding (text
  declarations of external entities), other platforms than those in
  "Platforms", other compilers, libexpat versions, or
  `XML_UNICODE` builds. (By reading the source only: `unknown_toUtf16`,
  xmltok.c 1304-1320, loops `while (*fromP < fromLim ...)`, so the same
  overshoot ends its loop; it was not tested.)
- Exploitability, severity, CVE status.

## The port

The Rust port (`crates/expat`, `#![forbid(unsafe_code)]`) converts like
`unknown_toUtf8` inside the data and stops at its end with
`InputIncomplete` instead of reading on; the parser hands processors only
`buffer[..buffer_end]`. Port-only bounds tests `tok::tests::q5` pass under
Miri (`docs/evidence/q5/miri.txt`, re-run in `rerun-main-a990a8c/miri.txt`).
The divergence (error position at the trailing byte, where C's result
depends on memory past the data) is DIVERGENCES.md Q-5 / D-035.

## Responsible-disclosure status

- **Not reported upstream yet.** No contact with the libexpat maintainers
  has been made; no CVE requested.
- libexpat's `SECURITY.md` asks for a private report by email to the
  maintainer and at least 90 days before public exposure.
- Note: this repository is public, and the finding is already visible in
  it (on `main` since PR #15: DIVERGENCES.md Q-5 / D-035 and
  `docs/evidence/q5`; and in PR #17 with this file). Whether and when to
  report privately, and whether to keep this material public until then,
  is the repository owner's decision; nothing here should be read as a
  coordinated disclosure.
- This file and `docs/evidence/q5` contain what the policy asks a report to
  include (description, how to reproduce); impact is listed under "What is
  not verified".
