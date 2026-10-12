# Divergences from libexpat 2.9.0

Every intentional difference between the port and the C oracle is listed here
with: the input that shows it (reproducer), the spec reference that justifies
the port's behaviour, the C behaviour, and the regression test that pins it.

The C library is the oracle; a divergence is accepted only when the C
behaviour is a bug relative to the XML 1.0 (Fifth Edition) / Namespaces in
XML 1.0 specifications or to libexpat's own documentation.

### DV-1 Allocation tracker: the port can refuse earlier than C (module 11c, D-030, D-034)

- Cause: the port cannot count libexpat's allocations (it does not
  allocate C's structures), so it charges at least C's bytes at C's
  allocation points or earlier (D-030). Its total exceeds C's by a margin
  `Δ(k) = N_port(k) − N_C(k)` at allocation event `k`. A document is
  refused by the port only when, at some event, the port's new total
  reaches the activation threshold with an amplification above the
  maximum while C's stays below at every event of its own parse.
- The margin has these components (measured on 64-bit Linux through the
  oracle, `docs/REPORT.md` DV-1 section):
  - the creation bound `CREATION_CHARGE` = 16,384 bytes against C's
    3,040 (root parser, 64-bit Linux and macOS; C's
    tracker is 32 bytes lower on Windows x64, so the margin is 32 bytes
    larger there) minus the C allocations the port does not
    replay one by one (measured: the namespace implicit context's pool
    block, 1,044 bytes, `m_atts` growth, the XML declaration strings);
    measured 11,764-13,352 bytes;
  - `ELEMENT_TYPE` charged as 96 bytes against C's 88: +8 bytes per
    element type, unbounded (grows with the number of element types);
  - `NS_ATT` charged as 24 bytes against C's 16 on Windows (4-byte
    `unsigned long`): +8 bytes per slot of `m_nsAtts` there;
  - the raw tag name charged together with the converted name when the
    tag opens (`Tracker::open_tag`); C stores it later and only when the
    input buffer moves (`storeRawNames`): grows with name length x depth;
  - **`POOL_SLACK` = 4**: a pool string is grown when fewer than
    `len + 4` characters are free, where C's `XmlConvert` fills a block to
    its last byte. When a string ends in the last four characters of a
    block that C fills exactly, the port doubles the block and C does not:
    the margin is **one whole pool block** (up to 32 MiB at the default
    limits), not a few kilobytes.
- Reproducers at the default limits (100.0, 64 MiB), in
  `tests/tests/dv1_windows.rs` (permanent regressions, D-034):
  - **W1 (creation charge):** `pad` spaces + `<!DOCTYPE r [<!ENTITY ee '`
    395 x `x` `'>]><r v='` + 85,000 x `&ee;` + `'/>`. For pad
    330,746-330,878 (documents of 671,180-671,312 bytes, 133 documents)
    C accepts (its last pool doubling reaches 67,117,916 bytes, amplification
    100.00) and the port refuses (33,576,836 + 33,554,432 = 67,131,268 >
    100 x direct). With 10,000 attribute defaults on an unused element type
    the same window lies at pad 188,323-188,456.
  - **W2 (pool slack):** 84,947 x `&ee;` + `tail` x `x`, no padding. For
    tail 363-366 (documents of 340,585-340,588 bytes) C fills its 32 MiB
    block exactly (33,554,431 characters and the NUL; end 33,563,484) and
    accepts; the port doubles to 64 MiB (amplification 197) and refuses.
  - With caller-tightened limits the window is easy to reach: at C's exact
    passing threshold the port refuses in 36 of 36 sweep cases
    (`tracker_diff.rs::threshold_boundary_sweep`, amplification 1.0).
- Justification: fail-closed security behaviour; the refusal is the same
  error C's tracker gives (`XML_ERROR_NO_MEMORY`), and the allocation
  tracker is a resource limit, not part of XML 1.0 well-formedness.
- The opposite direction (C refuses, the port accepts) is a security
  defect, not a divergence: it was found (R1 below) and fixed, and is
  tested by `tracker_sizes.rs` (every structure size >= C's `sizeof`),
  `dv1_windows.rs`, the corpus relations in `tracker_diff.rs` and the
  tracker relation of the `parse_trace` fuzz target.
- External entity parsers (module 11d step D2, D-032): every child
  charges the root's tracker. A general entity parser's charge is fully
  released when it is freed; a parameter entity parser keeps the shared
  DTD's growth charged (as in C) and releases its own structures. The
  creation-bound part of the margin stays 13,312-13,352 bytes whatever
  the number of children (87,381 nested parameter entity parsers: C
  8,028, port 21,380 bytes at the end;
  `external_parser_diff.rs::billion_laughs_across_external_entities`).
- **Status: being removed (owner decision 2026-10-12, D-034: option B,
  exact per-allocation parity, in stages).** Until every stage is merged,
  W1 and W2 remain and `dv1_windows.rs` pins them as "C accepts, port
  refuses". Rollback rule: if a parity stage ever produces C-refuses /
  port-accepts, `CREATION_CHARGE` returns to 16,384 and `POOL_SLACK` to 4
  (the size fixes stay).
- Regression tests: `tests/tests/dv1_windows.rs` (W1, W2, R1),
  `tracker_diff.rs::threshold_boundary_sweep` (36/36),
  `generated_documents_tracker_relation` and
  `upstream_vectors_tracker_relation` (no refusal at default limits where
  C accepts, on those corpora).

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
