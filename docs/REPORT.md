# Report

Honest status of the port. Numbers are measured, with the command that
produced them; nothing here is a projection.

## Target and why

libexpat 2.9.0 — see `docs/DECISIONS.md` D-001 (scoring table). Highest
security relevance and deployment footprint among permissive C parsers, and no
expat-compatible memory-safe implementation exists.

## Ported / unported

See the module table in `docs/STATE.md`. Ported and verified: modules 1-7
(byte tables, name predicates, encoders, SipHash, the tokenizer for every
built-in encoding and for application-defined encodings, the converters to
UTF-8/UTF-16, encoding detection and the XML declaration), i.e. all of
`xmltok.c`, `xmltok_impl.c` and `xmltok_ns.c`; module 8 is `xmlrole.c` (the
prolog/DTD state machine); module 9 is the string pool and hash table of
`xmlparse.c` with C's allocation behaviour. No public parser API exists yet.

## Verification infrastructure

- Oracle: vendored libexpat, byte-identical to the pinned commit
  (`scripts/verify-vendor.sh`, 39 files).
- Trace format = definition of "equal" (`docs/ARCHITECTURE.md`).
- 540 upstream-derived vectors (`tests/vectors/`), golden traces, 7 upstream
  error expectations asserted, 7 option sets in the differential matrix.

### Harness self-test

- Permanent: `tests/tests/harness_selftest.rs` compares the oracle against two
  deliberately wrong sources (off-by-one error column; dropped comment events)
  and asserts both are detected, and that the oracle agrees with itself.
- Deliberate bug in ported code (module 5, 2026-10-11): removing `BT_PERCNT`
  from the terminators of `scanPoundName` in `crates/expat/src/tok/scan.rs`
  made `tests/tests/tok_diff.rs::random_fragments_every_offset` fail on the
  first run (token/next mismatch reported with encoding, scanner, offset and
  input). The change was reverted.
- Deliberate bugs in ported code (modules 6-7, 2026-10-11), each reverted
  after the run: requiring a high instead of a low surrogate after a UTF-16
  lead (`utf16.rs`) failed `utf16_all_strings_up_to_three_units`; plane `+2`
  instead of `+1` in UTF-16→UTF-8 (`convert.rs`) failed
  `utf16_converters_all_strings_up_to_three_units`; name-start instead of
  name-char test in `unknown_isName` (`unknown.rs`) failed
  `unknown_encodings_seeded`; matching `yez` instead of `yes` in the XML
  declaration (`xmldecl.rs`) failed `xml_decl_in_every_builtin_encoding`;
  testing UTF-16BE instead of UTF-16LE in the `00 xx` detection rule
  (`init.rs`) failed `init_scan_all_strings_up_to_four_bytes`.
- Deliberate bug in ported code (module 8, 2026-10-11, reverted): `#FIXED`
  moving to `attlist1` instead of `attlist9` (`role/mod.rs`) failed
  `every_handler_token_and_keyword` and `dtd_constructs`.
- Deliberate bugs in ported code (module 9, 2026-10-11, each reverted):
  `poolClear` moving the blocks to a non-empty free list oldest first
  instead of newest first (`pool/mod.rs`) failed
  `pool_random_programs_match_c`; saturating instead of truncating
  `PROBE_STEP` to 8 bits (`hash/mod.rs`) failed `hash_large_tables_match_c`
  and `hash_random_programs_match_c`. (A first attempt at the second
  mutation was a no-op because `| 1` masked it; it was replaced.)

## Test and fuzz statistics

Measured 2026-10-11 on aarch64 Linux (2 cores), toolchain 1.99.0 / nightly
for fuzz and Miri.

- Modules 1-5 (PR #1): `cargo test --workspace --all-features`: 50 test
  functions + 5 doctests, all passing; `tok_diff.rs` ~34 s.
- Modules 6-7 (PR #2): `tests/tests/tok_diff.rs` has 12 tests (exhaustive
  strings over the byte-type and UTF-16 code-unit alphabets, converters with
  every output size 0-9 plus partial/ample on all strings of up to 3
  units/bytes, all byte pairs for the UTF-8 trimming, 300 seeded unknown
  encodings, the C rejection rules of `XmlInitUnknownEncoding`, upstream
  vectors natively and transcoded to UTF-16, 3,000 + 3,000 + 300 random
  documents); `tests/tests/init_diff.rs` has 4 (initScan on all 16,105
  strings of up to 4 bytes over the 11 bytes it dispatches on, at offsets 0
  and 1, for 11 names x 2 states x 2 NS modes; the upstream vectors; Q-3;
  4,000 generated declarations in all 12 built-in encodings plus seeded
  unknown encodings). Debug-build time of `tok_diff.rs` ~100 s on the
  2-core sandbox.
- Module 8 (PR #3): `tests/tests/role_diff.rs` has 4 tests (51 handlers x 49
  token kinds x 30 keyword texts x 16 state variants, single steps, in UTF-8
  and UTF-16BE: 2,399,040 comparisons; upstream vectors as document and
  external entity in 6 encodings; a DTD using every declaration form and
  conditional sections in all 12 built-in encodings; 2,000 random synthetic
  sequences, each also in a seeded unknown encoding), plus 2 unit tests.
- Whole workspace after module 8: `cargo test --workspace --all-features`
  runs 80 test functions (70 tests + 10 doctests), all passing, in 208 s
  (debug build, 2-core sandbox, with a fuzzer running on the other core;
  `tok_diff.rs` 163 s, `init_diff.rs` 24 s, `role_diff.rs` 17 s).
- `cargo +nightly miri test -p expat --all-features`
  (`-Zmiri-strict-provenance`): 20 unit tests + 5 doctests pass (97 s).
- Module 9 (PR #4): `tests/tests/core_diff.rs` has 9 tests (C's
  `poolBytesToAllocateFor` and `offsetof(BLOCK, s)` at every boundary;
  `hash()` for 2,000 keys/salts and the width of `unsigned long`; a pool
  scenario covering every function and growth strategy, run as is, with
  each of its first 40 allocation attempts refused, and with two refusals
  in a row before each operation (40 + 39 programs); growth up to the
  harness heap cap; 1,500 random pool programs; tables of up to 2^14 slots
  (6,000 keys, 3 salts); each of the first 80 allocation attempts of a
  70-key program refused, and refusals at the doubling; 1,500 random hash
  table programs), plus 7 unit tests (`pool`, `hash`).
- Whole workspace after module 9, with D-022: `cargo test --workspace
  --all-features` runs 99 test functions (86 tests + 13 doctests), all
  passing, in 57 s including the build (`core_diff.rs` 6.0 s,
  `tok_diff.rs` 8.3 s, `init_diff.rs` 4.4 s).
  `cargo +nightly miri test -p expat --all-features --lib -- pool hash`:
  the 10 pool/hash unit tests pass under Miri (21 s); the full Miri run is
  in CI.
- Fuzzing (`cargo +nightly fuzz run <target> -- -max_total_time=...`,
  AddressSanitizer on Rust and on the C oracle), no findings:

  | Target | Covers | Duration | Executions |
  |---|---|---|---|
  | `leaf_diff` | modules 1-4 vs C | 601 s | 6,876,903 |
  | `tok_diff` | module 5: all 6 scanners + 8 helpers vs C | 661 s | 527,795 |
  | `tok_diff` | modules 5-6: + UTF-16, seeded unknown encodings, converters | 661 s | 262,991 |
  | `tok_diff` | modules 5-7: + `initScan` (11 names x 2 states x 2 NS modes per input), `XmlParseXmlDecl` on every declaration token | 661 s | 233,945 |
  | `role_diff` | module 8: role + full state after every token, documents and synthetic sequences, document/external entity | 631 s | 5,158,518 |
  | `core_diff` | module 9: pool and hash table programs with allocation refusals, white-box state after every operation | 665 s (7 runs of 95 s) | 143,161 |

  Corpora were minimised with `cargo fuzz cmin` and committed
  (`fuzz/corpus/`). `core_diff` was run in 95-second slices because a
  command may run at most two minutes in the current sandbox; two earlier
  slices (216 s, 65,802 executions, no finding) are not counted because a
  deliberate-bug experiment edited the source while they were being built.

## Divergences

`docs/DIVERGENCES.md`. No divergences yet. Three C quirks found by the
harness, all chunking-related: Q-3 resolved as "replicate" (module 7; the
input violates XML 1.0 §4.3.3), Q-1 and Q-2 pending (module 10).
Platform-dependent C behaviour fixed to one choice: D-016 (counter width),
D-017 (sign of stray bytes in `utf8_toUtf16`).

## Benchmarks

Only an oracle baseline exists (`cargo bench -p expat-bench`); port-vs-C
comparisons start when the parser exists.

## Limitations

- No parser API yet (Phase 3 in progress).
- `XML_UNICODE` builds are out of scope (D-002).

## Reproduce

```sh
cargo test --workspace --all-features
scripts/ci-local.sh
cargo run -p expat-difftest --bin oracle_trace -- some.xml
```
