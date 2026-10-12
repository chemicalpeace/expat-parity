# Divergences from libexpat 2.9.0

Every intentional difference between the port and the C oracle is listed here
with: the input that shows it (reproducer), the spec reference that justifies
the port's behaviour, the C behaviour, and the regression test that pins it.

The C library is the oracle; a divergence is accepted only when the C
behaviour is a bug relative to the XML 1.0 (Fifth Edition) / Namespaces in
XML 1.0 specifications or to libexpat's own documentation.

### DV-1 Allocation tracker: the port could refuse earlier than C — removed by exact parity (D-034)

- **What it was (module 11c, D-030).** The port charged at least C's
  bytes at C's allocation points or earlier, so it could refuse
  (`XML_ERROR_NO_MEMORY`) where C did not: a creation upper bound of
  16,384 bytes against C's 3,040 (W1, a 133-document window of
  671 KB documents at the default limits), a pool slack of 4 characters
  that could cost a whole pool block (W2, 340 KB documents), per-element
  and per-slot over-counts (`ELEMENT_TYPE`, `NS_ATT` on Windows), the raw
  tag name charged too early. Two sizes below C's (R1, below) showed that
  the "at least C's bytes" premise did not even hold.
- **What replaced it (owner decision 2026-10-12, option B).** The port's
  tracker now replays C's allocations event by event (D-034, stages 1-8):
  sizes from C's field lists checked against the C compiler on every CI
  platform, an exact pool model, creation and `XML_ParserFree` as C's
  individual allocations, every C allocation the creation bound used to
  cover, the raw tag name at `storeRawNames`, and the refusal paths at
  C's state (C tolerates a refused `xmlns:` prefix, hands the entity
  declaration handler a partly stored value, keeps a tag whose buffer
  could not grow on the tag stack). The tests require equal streams
  (direction, size, refusal, running total, peak at every event) from
  creation through `XML_ParserFree`, equal traces and equal limit
  outcomes; the fuzz targets' tracker relation fails when the port
  refuses where C accepts, when C refuses where the port accepts, or when
  both refuse but stop differently (`tests/src/tracker.rs`,
  `compare_events`).
- **Status per platform:** see `docs/REPORT.md`, "DV-1 parity, stages
  2-8". aarch64 Linux (local, `e1a4e6e` and the merge with `main`):
  15,966 of 15,966 `tracker_events` streams equal, W1/W2/R1 width 0,
  the committed `parse_trace` corpus (13,869 inputs) and seeds pass the
  exact relation. The CI platforms (x86_64 Linux, aarch64 macOS, x86_64
  Windows) are recorded there from the PR's CI run; DV-1 is closed on a
  platform only when all of `tracker_events`, `tracker_stages`,
  `dv1_windows` and `tracker_sizes` pass on it.
- **Known remaining differences (none reachable by a document through
  the current API):** `copy_for_child` copies an external entity's base
  once per entity where C copies it once per run of entities sharing the
  same base pointer (an over-count, fail-closed; bases are always NULL
  until a base setter exists, module 12); `m_unknownEncodingMem` is not
  modelled (no unknown-encoding handler before module 12); the encoding
  name argument of `XML_ExternalEntityParserCreate` is charged like C's
  `copyString` but no test passes one (the harness's handlers pass NULL).
- Regression tests: `tests/tests/dv1_windows.rs` (W1, W2, R1 boundaries
  bisected per platform, now equal), `tracker_events.rs`,
  `tracker_stages.rs` (one named test per stage and per refusal path),
  `tracker_sizes.rs`, `tracker_diff.rs`, `fuzz_corpus.rs`.

### R1 Reverse violation: the port accepted documents C refuses (found and fixed 2026-10-12, D-034)

- **This was a security defect, not an accepted divergence.** Two
  structure sizes were charged below C's `sizeof` on 64-bit targets:
  `BINDING` 48 bytes against 56 (`size_t uriLen; size_t uriAlloc;`,
  `xmlparse.c` 262-270) and `NAME_AND_DEFAULT_ATTRIBUTE` 16 against 24
  (`size_t attIndex; bool initialized;`, 374-386). The port under-counted
  8 bytes per binding and per (element type, default attribute) entry,
  without bound, contradicting D-030's "at least C's bytes".
- Reproducer: the W1 document with
  `<!ATTLIST z a0 CDATA 'v' ... a9999 CDATA 'v'>` (10,000 defaults on an
  element type that never occurs), pad 187,657-188,322 (documents of
  686,993-687,658 bytes). C: `XML_ERROR_NO_MEMORY` (last doubling to
  68,765,848 bytes, amplification 100.10, `xmlparse.c:8608`). The port
  accepted (its total 68,699,208, 66,640 bytes below C's at that event).
- Why the tests missed it: their documents were small, so the
  16 KiB creation bound covered the deficit; the relation was only
  checked at the end of the parse, never per event.
- Fix (D-034): every tracked structure size is computed from C's field
  list with the target's C layout (`parse::c_layout`) and compared with
  the C compiler's `sizeof` on every CI platform
  (`tests/tests/tracker_sizes.rs`, an under-count always fails). Now C and
  the port both refuse R1 (`dv1_windows.rs::r1_attribute_defaults`).
  Injected bugs P1-P5 (each size reverted or shortened) are caught.

Platform-dependent C behaviour that the port fixes to one choice, without an
observable effect on parsing, is recorded in DECISIONS (D-016 counter width,
D-017 sign of stray bytes in `utf8_toUtf16`).

## Observed C quirks (candidates; decision due when the module is ported)

These are behaviours of libexpat 2.9.0 found by the harness that look like bugs
relative to the specification. The port is currently compared to C under the
same chunking, so they do not mask anything; each is resolved (replicate or
diverge with a regression test) when the owning module is ported. All three
are resolved as "replicate" (Q-1/Q-2: D-024, module 10; Q-3: D-019, module 7).

### Q-1 Epilog CR LF split across `XML_Parse` calls counts two lines — resolved: replicate

- Reproducer: `<a/>\r\n` fed in 1-byte chunks → `result ok 0 3 0 6`; fed whole
  → `result ok 0 2 0 6`. Same for upstream vectors
  `expat/tests/basic_tests.c:4615` (test_abort_epilog) and
  `basic_tests.c:456` (UTF-16LE, test_utf16_le_epilog_newline).
- Spec: XML 1.0 §2.11 — `#xD #xA` is one line break regardless of buffering.
- Cause: `epilogProcessor` consumes a CR at the end of the available input
  (a partial `XML_TOK_PROLOG_S`), so `XML_ParseBuffer` counts its line break;
  the LF in the next chunk is then a line break of its own.
- Decision (module 10, D-024): **replicate**. §2.11 is about the characters
  passed to the application, and none are passed for epilog white space
  either way; XML 1.0 does not specify reported positions. Only the position
  after the root element differs, never an event or an error.
- Regression tests: `tests/tests/parse_diff.rs::q1_q2_epilog_line_breaks_replicate_c`,
  `crates/expat/src/parse/tests.rs::epilog_cr_lf_split_counts_two_lines_like_c`.

### Q-2 Trailing CR at end of document: position depends on chunking — resolved: replicate

- Reproducer: `<a/>\r` whole → `result ok 0 1 4 4` (the CR is not consumed);
  chunked → `result ok 0 2 0 5`.
- Cause and decision: as Q-1 (D-024); the final call has no data, so the CR
  consumed by the previous chunk stays counted.
- Regression test: `tests/tests/parse_diff.rs::q1_q2_epilog_line_breaks_replicate_c`.

### Q-3 BOM-less UTF-16 detection with a 1-byte first chunk — resolved: replicate

- Reproducer: `basic_tests.c:188` (`" \0<\0e\0/\0>\0"`) whole → parses as
  UTF-16LE; 1-byte chunks → `result error 4 1 1 1` (invalid token), also
  `misc_tests.c:967`.
- Cause: `initScan` (`xmltok.c`) decides the encoding from a single byte when
  only one is available. A byte other than `00`, `3C`, `EF`, `FE`, `FF` selects
  the default encoding (UTF-8), so the following `00` is invalid; with two
  bytes, `20 00` selects UTF-16LE. libexpat documents this choice in the
  source ("we couldn't reliably determine whether we needed further bytes").
- Decision (module 7, 2026-10-11): **replicate**. XML 1.0 §4.3.3 requires an
  entity in UTF-16 to begin with a byte order mark; the reproducer has none
  and no external encoding, so it is not well-formed and rejecting it is
  correct. Appendix F (autodetection) is non-normative. Accepting it when it
  arrives in one piece is C's leniency, which the port keeps too, so the port
  equals C under every chunking. No divergence.
- Regression tests: `tests/tests/init_diff.rs::q3_single_byte_detection_matches_c`
  (detection on 1 byte, 2 bytes and the whole document, against C) and the
  chunk-dependence pin below.

### Q-4 Accounting of a name cut off by the end of the buffer in the epilog — resolved: do not replicate (undefined behaviour in C)

- Reproducer: `<r/>junk` (any chunking) with the accounting counters
  traced: C `accounting 18446462600611882832 0` (an address-dependent
  value that changes from run to run), port `accounting 4 0`; both
  `result error 9 1 4 4`.
- Cause: `epilogProcessor` (`xmlparse.c` 6610-6627) starts every token
  with `const char *next = NULL` and calls
  `accountingDiffTolerated(parser, tok, s, next, ...)` before looking at
  the token. For a name that reaches the end of the available input,
  `XmlPrologTok` returns `-XML_TOK_NAME` without writing `next`
  (`xmltok_impl.c` 1257, `return -tok;`), and `accountingDiffTolerated`
  only skips `XML_TOK_INVALID`, `XML_TOK_PARTIAL`, `XML_TOK_PARTIAL_CHAR`
  and `XML_TOK_NONE` (9020-9026). It computes `NULL - s` (undefined
  behaviour) and adds that negative difference to `countBytesDirect`,
  which wraps to about 2^64 minus the buffer address. The token is then
  rejected with `XML_ERROR_JUNK_AFTER_DOC_ELEMENT` (9); the parser is
  finished, so no later amplification check reads the corrupted counter.
  The overflow guard (9046-9048) would only turn this into error 43 if the
  counter exceeded the buffer address, which does not happen.
- Observable: only through `testingAccountingGetCountBytesDirect` and the
  `EXPAT_ACCOUNTING_DEBUG` output; the result, error code and position are
  equal.
- Decision (module 11b2, D-029): **do not replicate**. Undefined behaviour
  and an address-dependent value cannot be reproduced meaningfully; the
  port counts 0 bytes for the rejected token (its `next` defaults to the
  token start). The harness masks the `accounting` line of both traces
  exactly in this case (C result 9 and a direct counter >= 2^62,
  `expat_difftest::trace::mask_q4`); every other line is compared.
- Found by: the `parse_trace` corpus replay after the fuzz selector gained
  the accounting option (seven corpus inputs in UTF-8, UTF-16, US-ASCII
  and ISO-8859-1, all with junk after the root element).
- Regression test: `tests/tests/amplification_diff.rs::q4_epilog_name_at_end_of_buffer`.
- Upstream: worth reporting to libexpat (an `if (tok < 0) next = end;`
  or skipping negative tokens in `accountingDiffTolerated` would fix it);
  not reported from this project.

The exact set of chunk-dependent upstream vectors is pinned by
`tests/tests/upstream_vectors.rs::oracle_chunk_dependence_is_exactly_the_known_set`.

### Q-5 `toAscii` in `XmlParseXmlDecl` with an unknown encoding reads past `fromLim` and past the data — resolved: replicate inside the data, do not replicate the over-read (out-of-bounds read in C)

- Reproducers (random test `init_diff::xml_decl_in_every_builtin_encoding`,
  now pinned in `tests/tests/xmldecl_unknown_diff.rs`): seed
  `0x52f43d23123766f6`, `<?xml \xC3\xA9?><a/>` (map: `\xC3` lead of 2,
  `\xA9` lead of 4): C `bad` 8, port 7 before the fix; seed
  `0xfe8d9b29cbd0ff22`, `<?xml no\xC3\xA9 ?><a/>` (`\xC3` lead of 3,
  `\xA9` lead of 4): C 11, port 9. `bad` is `processXmlDecl`'s
  `&parser->m_eventPtr` (`xmlparse.c` 5143-5145), so it is the error's
  byte index: `XML_GetCurrentByteIndex64` = `m_parseEndByteIndex -
  (m_parseEndPtr - m_eventPtr)` (2714-2722). The column is a character
  count from `XmlUpdatePosition` (`XML_GetCurrentColumnNumber64`,
  2831-2840): C reports byte 8 / column 7 and byte 11 / column 10
  (`docs/evidence/q5/plain.txt`), `XML_ERROR_XML_DECL` (30) in both.
- Cause: `doParseXmlDecl` sets `end` to the `?` of `?>` (`xmltok.c`
  1069-1070); `parsePseudoAttribute` and `checkXmlDeclVersionNum` step by
  `minBytesPerChar` (one byte for an unknown encoding) and call
  `toAscii(enc, ptr, end)` (915-923) at every byte, including trailing
  bytes of a multi-byte character. `unknown_toUtf8` (1273-1301) stops
  only when `*fromP == fromLim` exactly (1281). When the map makes the
  trailing byte a lead byte, it passes the converter the whole sequence
  (1286) and advances past `fromLim` (1291-1292), after which the
  position never equals `fromLim` again; a sequence that converts to
  nothing (`XmlUtf8Encode` of a negative value is 0 bytes) does not fill
  the one-byte output, so C goes on with the following bytes until one
  produces output. The port stopped at `end`.
- **Upstream finding (out-of-bounds read).** When the declaration is at the
  end of the data, C reads past the data and, in a run of such sequences,
  past the allocation: `docs/evidence/q5/q5_overread.c` uses only the
  public API (unknown protocol encoding with an unknown-encoding handler,
  `XML_Parse` and `XML_GetBuffer`/`XML_ParseBuffer`) on both seed
  documents ending at `?>`. AddressSanitizer reports a
  heap-buffer-overflow READ in `unknown_toUtf8` (`xmltok.c` 1283) past the
  input buffer `XML_GetBuffer` allocated (`xmlparse.c` 2603), under
  `toAscii` / `parsePseudoAttribute` / `XmlParseXmlDecl` /
  `processXmlDecl` (`asan.txt`; the map makes ASan's heap fill byte
  `0xBE` a lead byte that converts to nothing). With a map that has only
  the two seed lead bytes, MemorySanitizer reports a use of uninitialised
  heap memory at `xmltok.c` 1284 (`msan.txt`), and the application's
  converter is called on bytes after the document (`plain.txt`). The bytes
  read are the parser's unused buffer space (stale or uninitialised) and,
  in the worst case, memory after the buffer. Requires an application
  that installs an unknown-encoding handler whose map makes a byte that
  can follow a lead byte a lead byte itself. Not reported upstream (the
  project contacts nobody); recorded for the owner.
- Decision (D-035): **replicate C inside the data, do not replicate the
  over-read.** `Encoding::to_utf8_until(buf, from, lim, to)` runs C's
  loop on the whole buffer with `fromLim` at `lim` (so it converts the
  bytes after `end` that C converts, which are inside the data); where C
  would read past the end of `buf` (a sequence not complete in `buf`, or
  the next byte after an empty conversion), it stops with
  `InputIncomplete` and `toAscii` gives `-1`: the declaration error is
  reported at that byte. The crate is `#![forbid(unsafe_code)]` and every
  access is a checked `get`, so the port cannot read outside the slice;
  the parser passes the processors `buffer[..buffer_end]`
  (`parse/mod.rs` `call_processor`), so in the parser the bound is the
  data the application gave, not the allocated buffer. The port's parser
  has no unknown-encoding handler yet (module 12); when it gets one this
  bound stays.
- Observable divergence: only where C reads outside the data, i.e. where
  C's result depends on indeterminate memory; there the port reports
  `XML_ERROR_XML_DECL` at the trailing byte (`bad` 7 for
  `<?xml \xC3\xA9?>` at the end of the data). Everywhere else the error
  code, byte index, line and column equal C's.
- Tests: `tests/tests/xmldecl_unknown_diff.rs` (C-differential: both
  seeds verbatim; 14 pseudo-attribute positions x 10 map kinds per byte x
  13 converter results x 3 tails, both NS modes and declaration kinds;
  the same declarations in all 12 built-in encodings and as whole parses
  under the option matrix; the port-only case pinned);
  `crates/expat/src/tok/tests.rs` `q5` (port-only bounds tests, run under
  Miri in CI).
