# Fuzzing and the committed corpora

Every fuzz target in `fuzz/fuzz_targets/` has a committed corpus in
`fuzz/corpus/<target>/`. The corpora are versioned test data: CI's fuzz
smoke job (`scripts/fuzz-smoke.sh`, 60 s per target) starts from them and
fails when a target has none, and `tests/tests/fuzz_corpus.rs` replays the
`parse_trace` and `buffer_diff` corpora through the same checks as the fuzz
targets (`expat_difftest::fuzzcase`) in the ordinary test suite, so every
CI platform (Linux, macOS, Windows) runs them.

## Toolchain used for the module 10 corpora

- `rustc 1.101.0-nightly (6f5da406b 2026-10-10)`, `cargo-fuzz 0.13.2`
  (libFuzzer from `libfuzzer-sys 0.4`), AddressSanitizer (cargo-fuzz
  default) on the Rust code and on the C oracle, aarch64 Linux.
- libFuzzer flags of every run: `-max_len=65536 -rss_limit_mb=2048`.

## Seeds (deterministic)

`expat_difftest::fuzzcase::seeds` is a pure function of the constant
`fuzzcase::SEED`; `seed_generator_is_deterministic` in
`tests/tests/fuzz_corpus.rs` checks it. The document generator
(`expat_difftest::docgen`) is a seeded xorshift generator, as are all the
generated-document tests.

```sh
cargo run -p expat-difftest --bin fuzz_seeds -- /tmp/seeds
# parse_trace: 6320 seeds   (540 upstream vectors x 8 selector pairs + 2,000 generated documents)
# buffer_diff: 2540 seeds   (540 upstream vectors + 2,000 generated documents, each with an operation header)
```

## Regenerating a corpus

A committed corpus is a libFuzzer merge (`-merge=1`, the same operation as
`cargo fuzz cmin`) of the previous committed corpus, the corpus a fuzzing
run grew, and the seeds:

```sh
cd fuzz
cargo +nightly fuzz run parse_trace /tmp/cmin/parse_trace \
    corpus/parse_trace /tmp/work/parse_trace /tmp/seeds/parse_trace \
    -- -merge=1 -max_len=65536 -rss_limit_mb=2048
rm -r corpus/parse_trace && cp -r /tmp/cmin/parse_trace corpus/parse_trace
# the same for buffer_diff
```

`/tmp/work/<target>` is the working corpus of a fuzzing run, e.g.
`cargo +nightly fuzz run parse_trace /tmp/work/parse_trace -- -max_total_time=95 -max_len=65536 -rss_limit_mb=2048 -print_final_stats=1`
(started from a copy of `corpus/parse_trace`). Given the same inputs and the
same build, the merge is deterministic; what a fuzzing run discovers is not
(libFuzzer picks a random `-seed` per run, recorded in each log), which is
why the merged result is committed rather than regenerated in CI.

Module 10 merges (2026-10-11). First: `parse_trace` 17,419 inputs (previous
committed corpus 1,080, minimised corpus of the earlier module 10 runs (7,719)
plus the units found by the last two logged slices, 5,780 seeds; the
working corpus of the first five logged slices was lost with a sandbox
reset) to 8,495 files;
`buffer_diff` 21,226 inputs (earlier and logged runs' corpus, 2,540 seeds) to
6,787 files. After the namespace-triplet option was added to the harness:
`parse_trace` committed corpus + 6,320 seeds to 8,444 files, then, after the
final 673-second run, committed corpus + that run's working corpus + seeds
(29,223 inputs) to 9,238 files (1,800,939 bytes, 10,658 edges in the
merge). `buffer_diff` is unchanged (its seeds did not change).

## Run logs

`docs/fuzz-logs/<module>/` keeps the libFuzzer output of the runs reported
in `docs/REPORT.md`, one file per slice (`<target>-<HHMMSS UTC>-<commit>.log`),
trimmed by removing the per-unit `NEW`/`REDUCE` lines (their count is in the
first line) and replacing local paths by `<fuzz>`, `<corpus>` and
`<artifacts>`. Each log ends with libFuzzer's `-print_final_stats` block
(`stat::number_of_executed_units`, `stat::new_units_added`, ...).
