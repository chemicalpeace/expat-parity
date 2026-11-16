# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Throughput on x86_64 Linux: manual workflow `bench.yml` and first figures (D-066).
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
  `parse_buffer_with_events`) and `error_code` (D-049), stop and resume
  (`Stopper`, `Parser::stopper`, `is_suspended`, `resume`,
  `resume_with`, `resume_with_events`, D-050), the default handler
  (`Events::default_data`, `Events::expand_internal_entities`,
  `Handler::default_data`, D-051), the text of each event
  (`Events::event_text`, `Handler::event_text`,
  `Stopper::mark_default_current`, D-052), the declaration events
  (`Handler::xml_decl`, `start_doctype_decl`, `end_doctype_decl`,
  `entity_decl`, `unparsed_entity_decl`, `notation_decl`, `element_decl`,
  `attlist_decl`, `skipped_entity`, the matching `Events` flags,
  `ContentModel` and its parts) and `Parser::set_base` / `base` (D-053),
  the parser settings (`set_salt`, `set_encoding`,
  `set_return_ns_triplet`, `use_foreign_dtd`, `set_param_entity_parsing`
  with `ParamEntityParsing`, the billion laughs and allocation tracker
  limits, `set_reparse_deferral_enabled`, with their getters),
  `parsing_status` with `ParsingStatus`, `is_final_buffer`, `reset`,
  the same limits and status on `Stopper` for use inside handlers, and
  `Handler::take_base` (D-054), external entities
  (`Events::external_entity_ref`, `Handler::external_entity_ref`,
  `ExternalEntities`, `ExternalParser`) and `Handler::not_standalone`
  (D-055), `Events::input_context` with `Handler::input_context`,
  `Events::id_attribute` with `Handler::id_attribute`,
  `Parser::current_byte_count` and `Parser::input_context` (D-056),
  `Handler::unknown_encoding` with `EncodingInfo` (D-057).
- `crates/expat-capi` (not published): a first part of libexpat's C
  interface over the crate, compared with libexpat by
  `scripts/capi-check.sh` (D-047 to D-057).
- Allocation failure injection: the oracle's memory suite can fail from
  the n-th call on, and the port fails at the same C allocation through
  its tracker; every allocation point is compared (`oom_diff.rs`, D-058).
- The W3C XML Conformance Test Suite (xmlts20130923, not vendored) is
  compared with libexpat: 2,586 documents under five option sets
  (`w3c_diff.rs`, `scripts/w3c-check.sh`, D-059).
- Drop-in shared library (`scripts/build-dropin.sh`, `libexpat.so.1`) and
  libexpat 2.10.0's own test suite run against it
  (`scripts/runtests-check.sh`): every test passes except 12 that call
  libexpat internals (D-060).
- `docs/ROADMAP.md`: status and planned work against the points in
  "What a rewrite of libexpat needs" (libexpat maintainer, 2026-11-10).
- Prepared for publication as `expat` `0.1.0-beta.1` (experimental): the
  crate now carries `LICENSE` and `NOTICE`, and the README documents usage
  and limitations. Not yet published.
- `scripts/doc-limits.sh` and `benches/docs/measure.c`: large benign
  DocBook and DITA documents measured against the default limits, C and
  port compared (D-063).
- `docs/RELEASING.md`, the procedure for following a libexpat release,
  and `scripts/upstream-check.sh` with a weekly workflow that compares
  the port with libexpat's development branch (D-062).
- `Parser::try_with_options` and `Options::try_encoding`: creating a
  parser reports a failure of the system allocator instead of aborting.
  Through the C interface, `XML_ParserCreate`, `XML_ParserCreateNS`,
  `XML_ParserCreate_MM` and `XML_ExternalEntityParserCreate` return
  `NULL`, `XML_SetEncoding` and `XML_SetBase` return `XML_STATUS_ERROR`
  and `XML_ParserReset` returns `XML_FALSE` in that case (D-065).
- `Parser::set_fifth_edition_names`: an option, off by default and not
  part of libexpat, that classifies name characters by XML 1.0 Fifth
  Edition instead of the Fourth Edition classes libexpat uses (D-064).
- `crates/expat-capi/tests/system_oom.rs`: fails each system
  allocation in turn while parsing and checks that none aborts (D-061).

### Fixed
- A failure of the system allocator while parsing ends the parse with
  `ErrorCode::NoMemory` instead of aborting the process (D-061).
- C interface: `XML_ParserFree` from an unknown encoding's `release`
  function freed the parser twice; external entity parsers can be created
  outside a handler; the amplification setters refuse external entity
  parsers; `XML_ParserCreate_MM` routes every allocation libexpat makes
  through the program's memory functions (D-060).

### Changed
- `Parser::reset` returns whether the parser was reset (`false` for an
  external entity parser, D-055).
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
- The parser never loads external entities itself. Allocation failure
  while creating, resetting or configuring a parser aborts (D-061). See
  the README.
