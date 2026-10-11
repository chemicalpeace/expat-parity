# Report

Honest status of the port. Numbers are measured, with the command that
produced them; nothing here is a projection.

## Target and why

libexpat 2.9.0 — see `docs/DECISIONS.md` D-001 (scoring table). Highest
security relevance and deployment footprint among permissive C parsers, and no
expat-compatible memory-safe implementation exists.

## Ported / unported

See the module table in `docs/STATE.md`. Ported and verified: modules 1-5
(byte tables, name predicates, encoders, SipHash, the tokenizer for the
single-byte encodings). No public parser API exists yet.

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

## Test and fuzz statistics

Measured 2026-10-11 on aarch64 Linux (2 cores), toolchain 1.99.0 / nightly
for fuzz and Miri.

- `cargo test --workspace --all-features`: 50 test functions + 5 doctests,
  all passing. `tok_diff.rs` alone runs ~34 s (all 1 + 56 + 56² + 56³ strings
  over the byte-type alphabet x 8 encodings x 6 scanners; token walks and
  every-offset checks of the 540 upstream vectors; 3,000 random fragment
  documents).
- `cargo +nightly miri test -p expat --all-features`
  (`-Zmiri-strict-provenance`): 20 unit tests + 5 doctests pass (97 s).
- Fuzzing (`cargo +nightly fuzz run <target> -- -max_total_time=...`,
  AddressSanitizer on Rust and on the C oracle), no findings:

  | Target | Covers | Duration | Executions |
  |---|---|---|---|
  | `leaf_diff` | modules 1-4 vs C | 601 s | 6,876,903 |
  | `tok_diff` | module 5: all 6 scanners + 8 helpers vs C | 661 s | 527,795 |

  Corpora were minimised with `cargo fuzz cmin` and committed
  (`fuzz/corpus/`).

## Divergences

`docs/DIVERGENCES.md`. No divergences yet; three C quirks (Q-1..Q-3) found by
the harness, all chunking-related, pending decision.

## Benchmarks

Only an oracle baseline exists (`cargo bench -p expat-bench`); port-vs-C
comparisons start when the parser exists.

## Limitations

- No parser API yet (Phase 3 in progress).
- The tokenizer covers the single-byte encodings only; UTF-16 and unknown
  encodings are module 6.
- `XML_UNICODE` builds are out of scope (D-002).

## Reproduce

```sh
cargo test --workspace --all-features
scripts/ci-local.sh
cargo run -p expat-difftest --bin oracle_trace -- some.xml
```
