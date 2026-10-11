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
same chunking, so they do not mask anything; each will be resolved (replicate
or diverge with a regression test) when the owning module is ported.

### Q-1 Epilog CR LF split across `XML_Parse` calls counts two lines

- Reproducer: `<a/>\r\n` fed in 1-byte chunks → `result ok 0 3 0 6`; fed whole
  → `result ok 0 2 0 6`. Same for upstream vectors
  `expat/tests/basic_tests.c:4615` (test_abort_epilog) and
  `basic_tests.c:456` (UTF-16LE, test_utf16_le_epilog_newline).
- Spec: XML 1.0 §2.11 — `#xD #xA` is one line break regardless of buffering.
- Owning module: `xmlparse.c` `epilogProcessor` / `XmlUpdatePosition` with
  `XML_TOK_TRAILING_CR`. Not yet ported.

### Q-2 Trailing CR at end of document: position depends on chunking

- Reproducer: `<a/>\r` whole → `result ok 0 1 4 4` (the CR is not consumed);
  chunked → `result ok 0 2 0 5`.
- Owning module: same as Q-1.

### Q-3 BOM-less UTF-16 detection with a 1-byte first chunk

- Reproducer: `basic_tests.c:188` (`" \0<\0e\0/\0>\0"`) whole → parses as
  UTF-16LE; 1-byte chunks → `result error 4 1 1 1` (invalid token), also
  `misc_tests.c:967`.
- Spec: XML 1.0 Appendix F (autodetection needs the first bytes; a streaming
  parser should defer the decision rather than guess). Owning module:
  `xmltok.c` `initScan`. Not yet ported.

The exact set of chunk-dependent upstream vectors is pinned by
`tests/tests/upstream_vectors.rs::oracle_chunk_dependence_is_exactly_the_known_set`.
