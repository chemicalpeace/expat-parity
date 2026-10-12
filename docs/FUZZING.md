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
# parse_trace: 10320 seeds   (540 upstream vectors x 8 selector pairs + 2,000 generated documents
#                            + 2,000 documents with a DTD (11a) + 1,500 entity documents (11b)
#                            + 500 entity documents with small billion-laughs limits (11b2))
# buffer_diff: 4040 seeds   (540 upstream vectors + 2,000 generated documents + 1,000 documents
#                            with a DTD + 500 entity documents, each with an operation header)
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

Module 11a merges (2026-10-12): `parse_trace` committed corpus (9,238) +
working corpus of the logged runs + 8,320 seeds = 41,905 inputs to 10,647
files (13,750 edges); `buffer_diff` 6,787 + working corpus + 3,540 seeds =
32,718 inputs to 9,674 files (12,256 edges). The working corpora were kept
on local disk and the fuzz targets built in a local-disk copy of the
repository (the workspace file system is slow for many small files).

Module 11b merges (2026-10-12): `parse_trace` committed corpus (10,647) +
working corpus of the logged runs (16,373) + 9,820 seeds = 36,840 inputs
to 11,036 files (14,108 edges); `buffer_diff` 9,674 + 15,515 + 4,040 =
29,229 inputs to 10,208 files (12,551 edges). Before committing, both
merged corpora were searched for personal data (names, e-mail addresses,
local paths); no hits.

Module 11b2 merge (2026-10-12): `parse_trace` committed corpus (11,036) +
working corpus (14,646) + 10,320 seeds (now including 500 entity documents
with small billion-laughs limits) = 36,002 inputs to 11,174 files (14,355
edges); scanned for personal data, no hits. `buffer_diff` unchanged.

Module 11c merges (2026-10-12): `parse_trace` committed corpus (11,174) +
working corpus of the logged runs (15,907) + 10,320 seeds = 37,401 inputs
to 12,638 files (15,054 edges); `buffer_diff` 10,208 + 13,709 + 4,040 =
27,957 inputs to 10,451 files (12,866 edges). The target now also checks
the allocation-tracker relation (`expat_difftest::tracker::check_from`).
Scanned for personal data (names, e-mail addresses, local paths): no hits.
An earlier step-C run (commit 7762e9d) and its merged corpus were lost
with a sandbox reset before they were pushed; its numbers are not used.

Module 11d step D2 merges (2026-10-12): `parse_trace` committed corpus
(12,638) + working corpus of the first logged run (27,310, incl. 12,120
seeds of which 2,300 carry external entity bodies after
`EXTERNAL_MARKER`) = 39,948 inputs to 13,555 files (1,986 new edges), then
13,555 + the working corpus of the run after the PE-parser fix (15,709) =
29,264 inputs to 13,869 files (106 new edges); `buffer_diff` 10,451 +
17,792 (incl. 4,040 seeds) = 28,243 inputs to 10,766 files (31 new
edges). Scanned for personal data (names, e-mail addresses, local paths;
digit runs checked by hand): no hits.

## Run logs

`docs/fuzz-logs/<module>/` keeps the libFuzzer output of the runs reported
in `docs/REPORT.md`, one file per slice (`<target>-<HHMMSS UTC>-<commit>.log`),
trimmed by removing the per-unit `NEW`/`REDUCE` lines (their count is in the
first line) and replacing local paths by `<fuzz>`, `<corpus>` and
`<artifacts>`. Each log ends with libFuzzer's `-print_final_stats` block
(`stat::number_of_executed_units`, `stat::new_units_added`, ...).
