# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Phase 0: target selection (libexpat 2.9.0), frozen in `docs/DECISIONS.md`.
- Phase 1: workspace, vendored C oracle, CI, debt check, documentation skeleton.
- Phase 2: oracle FFI, canonical trace format, differential harness, test
  vectors extracted from the upstream test suite, fuzz targets, harness
  self-test.
- Phase 3: the parser core ported bottom-up (modules 1-11: byte tables,
  name predicates, encoders, SipHash, tokenizer in every built-in
  encoding, prolog state machine, string pool and hash tables, parser
  state and buffer, content, attributes, namespaces, DTD, internal and
  external entity parsing, amplification limits, allocation tracker with
  exact parity to libexpat), each compared with the C oracle; see
  `docs/STATE.md` and `docs/REPORT.md`.
- Public API (module 12, the MVP): `Parser` (`new`, `with_options`,
  `parse`, `parse_with`), `Options` (encoding name, namespace separator),
  `Error` and `ErrorCode` with libexpat's texts, `Handler` with nine
  events and `Events` (D-038 to D-041).
- Prepared for publication as `expat` `0.1.0-beta.1` (experimental): the
  crate now carries `LICENSE` and `NOTICE`, and the README documents usage
  and limitations. Not yet published.

### Known limitations
- No default handler, DTD or entity declaration events, unknown-encoding
  handler, position in callbacks, or stop/resume; external entities are
  never loaded; real allocator failure aborts instead of returning
  `NoMemory`. See the README.
