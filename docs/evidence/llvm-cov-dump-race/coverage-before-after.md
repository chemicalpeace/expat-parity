# `-num-threads=1` changes only the -dump diagnostic: CI before/after

The change (ec9cf645) touches one line of `scripts/coverage.sh`: the
`LLVM_COV_FLAGS="-dump ..."` invocation, whose stdout goes to `/dev/null`
and whose stderr is reduced to the `hash-mismatch:` list
(`<report>-mismatch.txt`). The summary, JSON, lcov and missing-lines
reports are made by four separate `cargo llvm-cov report` calls that do not
get the flag. To check this on the outputs, the `coverage-<OS>` artifacts
and coverage-job logs of three CI runs were compared with
`compare_coverage.py` (full output: `coverage-before-after-output.txt`):

| label | run | commit | `-num-threads=1` |
|---|---|---|---|
| before | 38240812899 (post-merge `main`) | a990a8ca | no |
| d36 | 38246973844 (PR #17) | d36cebe5 | yes |
| 98d | 38248323590 (PR #17) | 98d87c5e | yes (Linux job not finished when this was written) |

`crates/expat` sources are identical in all three; between a990a8ca and the
PR, the test code differs only by an `eprintln!` in
`tests/tests/xmldecl_unknown_diff.rs`.

## Results, per platform

| | Linux | macOS | Windows |
|---|---|---|---|
| totals, all 4 reports (lines / regions / functions) | 7218/7609, 12885/13551, 570/616 in before and d36: identical | 7218/7609, 12885/13551, 570/616 in all three: identical | 7213/7606, 12876/13546, 569/615 in all three: identical |
| hash-mismatch list (the -dump output) | identical | identical | identical |
| tests run in the coverage job (names, multiplicity, passed/failed/ignored) | 208 ok, 0 failed, 0 ignored: identical set and multiset | 208 / 0 / 0: identical | 206 / 0 / 0: identical |
| lcov line (DA) and branch (BRDA) records, covered yes/no | identical | identical | identical |
| lcov function (FNDA) records, covered yes/no | identical | 2 records differ between any two runs | 2 records differ (before = 98d; d36 differs) |
| lcov hit counts | differ | differ | differ |

The FNDA and hit-count differences are not caused by the flag: they occur
equally between d36 and 98d, which both have it. Every differing FNDA
record is a monomorphised instance of `tok::scan::is_public_id` in the
`init_diff` test binary (e.g. `is_public_id<Unknown<TableConverter>>`),
covered or not depending on the cases the unseeded proptest
`init_diff::xml_decl_in_every_builtin_encoding` (4,000 random cases in this
job) happens to draw; the function itself is covered in every run, so the
totals do not move. Hit counts vary for the same reason (and llvm-cov's
function order in lcov varies between runs, so the comparison sorts
records).

Conclusion: coverage numbers (lines, regions, functions, per platform), the
line/branch coverage map, the mismatch list and the set of tests run are
identical before and after; the only differences are run-to-run variance
from an unseeded random test, present with the flag on both sides.
