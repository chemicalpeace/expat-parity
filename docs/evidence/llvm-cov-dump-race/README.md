# llvm-cov `report -dump` heap corruption: root cause

## What happened

CI run 38237867506 (PR #15, head 6788ba52), attempt 1, job "coverage
(ubuntu-latest)": every test passed and every report was written, then the
`-dump` diagnostic of `scripts/coverage.sh` (the second, corrected
`all-features` run) died with SIGABRT:

```
warning: 225 functions have mismatched data
PLEASE submit a bug report to https://github.com/llvm/llvm-project/issues/ ...
corrupted size vs. prev_size in fastbins
error: failed to generate report: process didn't exit successfully: `.../1.99.0-x86_64-unknown-linux-gnu/.../llvm-cov report ... -dump` (signal: 6 (SIGABRT) (core dumped))
```

(`ci-38237867506-attempt1-dump-stderr.txt`: the stderr of that command
without its 225 `hash-mismatch:` lines, from the job's `coverage-Linux`
artifact.) All 225 `hash-mismatch` lines were printed before the abort,
so loading the coverage data had finished; the crash came in the report
phase. The re-run of the same commit (job 114777514198) was green.
Toolchain: rustc 1.99.0, LLVM 23.1.1, cargo-llvm-cov 0.9.1.

## Root cause

A data race inside llvm-cov, not our data. With `-dump`
(`ViewOpts.Debug`), `llvm-cov report` prints a debug line per function
group from `CoverageReport::prepareSingleFileReport`
(`llvm/tools/llvm-cov/CoverageReport.cpp`, current `main`):

```cpp
    if (Options.Debug)
      outs() << "InstantiationGroup: " << GroupSummary.Name << " with "
             << "size = " << Group.size() << "\n";
```

`prepareSingleFileReport` runs once per source file on a thread pool
(`prepareFileReports`: `Pool.async(&CoverageReport::prepareSingleFileReport, ...)`,
one thread per file up to the core count when `-num-threads` is not
given). `outs()` is one global `raw_fd_ostream`, which is not thread-safe:
concurrent writes race on its buffer pointers (`OutBufStart`, `OutBufCur`,
`OutBufEnd`, set in `raw_ostream::SetBufferSize` / `write`), so bytes land
outside the buffer and corrupt the heap. The line was moved into the
thread-pool task when the report was parallelised (LLVM review D41206,
commit cc254ba4a7ed, January 2018, https://reviews.llvm.org/D41206) and is
unchanged in LLVM `main` (fetched 2026-10-12). No upstream issue for it
was found (search on the LLVM issue tracker and the web; not filed).

Why it is intermittent: it needs two worker threads inside `operator<<` on
`outs()` at the same moment, which depends on scheduling; most runs pass.

## Evidence (aarch64 Linux, rustc 1.99.0 / LLVM 23.1.1-rust-1.99.0-stable)

`stress.sh` repeats `llvm-cov report` on a small instrumented build (the
`entity_diff` and `xmldecl_unknown_diff` test targets of main a990a8c,
crates/expat and its dependents) for a fixed time and counts runs that do
not exit 0. `race.profdata` is the plain merged profile;
`race-fix.profdata` adds the records of `scripts/coverage_fix_profdata.py`
(405 names), exactly as `scripts/coverage.sh` does. Results
(`results.txt`, verbatim):

| Run | Profile | Flags | Runs | Failures |
|---|---|---|---|---|
| dump-j8 | plain | `-dump -num-threads=8` | 1,955 | **13** (exit 134 SIGABRT x6, 139 SIGSEGV x3, 1 x3 "LLVM ERROR: IO failure on output stream: Bad address", 124 hang x1) |
| dump-j1 | plain | `-dump -num-threads=1` | 1,387 | 0 |
| nodump-j8 | plain | `-num-threads=8` | 1,426 | 0 |
| fix-dump-j8 | with fix records | `-dump -num-threads=8` | 520 | **2** (124 hang x2; each hang costs the 30 s timeout) |
| fix-dump-j1 | with fix records | `-dump -num-threads=1` | 1,403 | 0 |

`failures/` holds the stderr of every failing run (glibc messages such as
`malloc(): unaligned tcache chunk detected`, `free(): invalid size`,
`free(): invalid next size (normal)`, and LLVM stack dumps whose crashing
thread runs in `llvm::StdThreadPool::processTasks`). An earlier session
of the same experiment (lost in a sandbox reset before its logs were
saved) gave 6/2,138, 0/1,437, 0/1,487, 3/1,148 and 0/1,443 for the same
five configurations.

Conclusions the data supports:

- the failure needs `-dump` and more than one report thread: with either
  removed, 5,643 runs had no failure;
- it happens with the plain merged profile, so
  `scripts/coverage_fix_profdata.py` is not the cause. Its output merges
  cleanly (`llvm-profdata merge` validates the text records) and loads
  without failure in 1,403 single-threaded `-dump` runs.

`helgrind-dump.txt` / `helgrind-nodump.txt`: `valgrind --tool=helgrind`
(3.24.0) on one `llvm-cov report -num-threads=8` run with and without
`-dump`. With `-dump`: 71 "Possible data race" reports, 30 of them with
`llvm::CoverageReport::prepareSingleFileReport` -> `llvm::raw_ostream::write`
-> `raw_ostream::SetBufferSize` / `raw_fd_ostream::write_impl` on the top
of the stack, "Locks held: none" (first one quoted verbatim). Without
`-dump`: 7 reports, none in `prepareSingleFileReport`; they are in the
main thread destroying the futures after `Pool.wait()`, where the
synchronisation goes through `std::future`, which helgrind does not model
(expected false positives; not investigated further).

Not verified: the x86_64 CI runner's exact interleaving (the crash there
could not be re-triggered on demand, as it is intermittent), and whether
the nightly llvm-cov abort noted in `scripts/coverage.sh` is the same
race.

## Fix

`scripts/coverage.sh` runs the `-dump` diagnostic with
`LLVM_COV_FLAGS="-dump -num-threads=1"`. cargo-llvm-cov appends
`LLVM_COV_FLAGS` to its `llvm-cov report` command line (checked with
`cargo llvm-cov report -vv`); with one thread the per-file tasks run one
after another, so the unsynchronised writes cannot overlap. The diagnostic
keeps its purpose (the full list of hash mismatches) and the mismatch check
is unchanged; the four reports (`summary`, `missing-lines`, `json`, `lcov`)
are produced by separate commands without `-dump` and keep the default
threading. No step is skipped, made non-fatal, or retried.

## Reproduce

```sh
docs/evidence/llvm-cov-dump-race/stress.sh prepare
docs/evidence/llvm-cov-dump-race/stress.sh fixprof
docs/evidence/llvm-cov-dump-race/stress.sh run dump-j8 100 race.profdata -dump -num-threads=8
docs/evidence/llvm-cov-dump-race/stress.sh run dump-j1 100 race.profdata -dump -num-threads=1
docs/evidence/llvm-cov-dump-race/stress.sh run nodump-j8 100 race.profdata -num-threads=8
docs/evidence/llvm-cov-dump-race/stress.sh run fix-dump-j8 100 race-fix.profdata -dump -num-threads=8
docs/evidence/llvm-cov-dump-race/stress.sh run fix-dump-j1 100 race-fix.profdata -dump -num-threads=1
```

Failure rates depend on the machine (2 cores here); the counts above are
from one run each. The last four configurations ran at the same time on
the same machine (four concurrent `stress.sh` processes), so their run
counts are lower than a run alone would give; `results.txt` lists the
lines in the order they finished.
