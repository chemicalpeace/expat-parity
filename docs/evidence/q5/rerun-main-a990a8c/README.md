# Q-5 evidence re-run on main a990a8c (2026-10-12)

PR #15 was merged as main a990a8c. These files repeat the checks on that
library code (branch q5-verification; the branch adds no library change),
Linux aarch64, Debian clang 19.1.7:

| File | Command | Result |
|---|---|---|
| `plain.txt`, `asan.txt`, `msan.txt` | `OUT=/tmp/q5 docs/evidence/q5/build.sh && docs/evidence/q5/run.sh && docs/evidence/q5/check.sh` | same as `../` (020a0618) apart from addresses and the commit line: 4 ASan heap-buffer-overflow READs in `unknown_toUtf8` xmltok.c:1283 (block from `XML_GetBuffer` xmlparse.c:2603), 2 MSan use-of-uninitialized-value at xmltok.c:1284; `check.sh` exit 0 |
| `miri.txt` | `MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri test -q -p expat --all-features --lib q5` | 2 passed |
| `random-scan.txt` | release `init_diff` binary, `--exact xml_decl_in_every_builtin_encoding`, `PROPTEST_CASES=100000 PROPTEST_RNG_SEED=11` and `95000` / `12` (195,000 cases), plus 1,000 and 10,000-case controls (run time scales linearly, so the case count is applied) | all ok |

The 34,944-case C-differential test (`xmldecl_unknown_diff`) now prints its
count: `xmldecl_unknown_diff: 34944 declarations compared, 468 rejected
pairs checked` (local release run on this branch; CI prints it per
platform in the step "Q-5 differential count and seeded random runs").
