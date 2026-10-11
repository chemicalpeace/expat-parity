# Divergences from libexpat 2.9.0

Every intentional difference between the port and the C oracle is listed here
with: the input that shows it (reproducer), the spec reference that justifies
the port's behaviour, the C behaviour, and the regression test that pins it.

The C library is the oracle; a divergence is accepted only when the C
behaviour is a bug relative to the XML 1.0 (Fifth Edition) / Namespaces in
XML 1.0 specifications or to libexpat's own documentation.

_No divergences yet._

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

The exact set of chunk-dependent upstream vectors is pinned by
`tests/tests/upstream_vectors.rs::oracle_chunk_dependence_is_exactly_the_known_set`.
