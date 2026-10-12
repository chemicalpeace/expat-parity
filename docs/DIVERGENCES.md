# Divergences from libexpat 2.9.0

Every intentional difference between the port and the C oracle is listed here
with: the input that shows it (reproducer), the spec reference that justifies
the port's behaviour, the C behaviour, and the regression test that pins it.

The C library is the oracle; a divergence is accepted only when the C
behaviour is a bug relative to the XML 1.0 (Fifth Edition) / Namespaces in
XML 1.0 specifications or to libexpat's own documentation.

### DV-1 Allocation tracker: the port can refuse slightly earlier than C (module 11c, D-030)

- Reproducer: any document under tightened tracker limits at C's exact
  passing threshold, e.g. the sweep document "deep nesting" in
  `tests/tests/tracker_diff.rs` with `XML_SetAllocTrackerMaximumAmplification(1.0)`
  and `XML_SetAllocTrackerActivationThreshold(t)`, where `t` is the
  smallest threshold at which C succeeds: C parses the document, the port
  stops with `XML_ERROR_NO_MEMORY` (36 of 36 sweep documents x option
  sets).
- Cause: the port cannot count libexpat's allocations (it does not
  allocate C's structures), so it charges at least C's bytes at C's
  allocation points or earlier (D-030); its total exceeds C's by a margin
  (measured 11,764-13,352 bytes, mostly the 16 KiB creation bound against
  C's 3,040-3,059 bytes). Documents whose C total ends within that margin
  below the refusal point are refused by the port only.
- At the default limits (100.0, 64 MiB) the window requires a tracked
  total of at least 64 MiB within the margin below 100 times the direct
  input; no test, vector or fuzz input reached it (the tests assert that
  the port never refuses at the default limits where C accepts).
- Justification: fail-closed security behaviour; the refusal is the same
  error C's tracker gives (`XML_ERROR_NO_MEMORY`), and the allocation
  tracker is a resource limit, not part of XML 1.0 well-formedness.
  Exact replication would require C's structure layouts and every
  incidental allocation (D-030).
- The opposite direction never happens by construction and is tested:
  whenever C refuses through its tracker, the port refuses
  (`tests/tests/tracker_diff.rs`, `parse_trace` fuzz target).
- **Status: pending owner decision** (2026-10-12). Whether the ~13 KB
  window above 64 MiB at the default limits is acceptable, or the port
  must be brought to C's exact count there, is open; the behaviour is
  left unchanged until decided.
- Regression tests: `tests/tests/tracker_diff.rs::threshold_boundary_sweep`
  (asserts the 36/36 count), `generated_documents_tracker_relation` and
  `upstream_vectors_tracker_relation` (no refusal at default limits where
  C accepts).

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
