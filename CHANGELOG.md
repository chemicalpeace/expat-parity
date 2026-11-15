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
- Public API, continued: `Parser::parse_with_events` (handlers chosen at
  run time), the position between calls (`current_line`,
  `current_column`, `current_byte_index`), event positions for handlers
  (`Events::position`, `Handler::position`, D-048), the buffer functions
  (`buffer`, `parse_buffer`, `parse_buffer_with`,
  `parse_buffer_with_events`) and `error_code` (D-049).
- `crates/expat-capi` (not published): a first part of libexpat's C
  interface over the crate, compared with libexpat by
  `scripts/capi-check.sh` (D-047 to D-049).
- Prepared for publication as `expat` `0.1.0-beta.1` (experimental): the
  crate now carries `LICENSE` and `NOTICE`, and the README documents usage
  and limitations. Not yet published.

### Changed
- Target and C oracle moved from libexpat 2.9.0 to 2.10.0 (D-042). The
  vendored sources are re-vendored byte for byte; the port follows every
  2.10.0 change that is observable: an entity declaration whose value
  fails is no longer reported (#1415), an empty first input of an
  external entity parser keeps the byte order mark and text declaration
  detection (#1444), attribute counts above `INT_MAX` are refused (#1450).

### Security
- CVE-2026-115506 (billion-laughs variants): the expanded size of an
  internal entity is computed on its first expansion and a reference
  whose expansion would exceed the amplification limits is refused before
  it is expanded, as in libexpat 2.10.0. A crafted document with 8 MiB of
  padding is now rejected in about 15 ms instead of about 30 s.

### Known limitations
- No default handler, DTD or entity declaration events, unknown-encoding
  handler, or stop/resume; external entities are
  never loaded; real allocator failure aborts instead of returning
  `NoMemory`. See the README.
