# Roadmap

Status of the port against the points listed by the libexpat maintainer in
"What a rewrite of libexpat needs" (Sebastian Pipping, 2026-11-10,
<https://blog.hartwork.org/posts/what-a-libexpat-rewrite-needs/>), and the
work planned for each. The reference is libexpat 2.10.0. Figures are from
the decisions cited; `docs/STATE.md` has the current state and
`docs/REPORT.md` the evidence.

Status words: **done** (implemented and compared with libexpat),
**partial** (some of it done, the rest listed), **open** (not started).

| # | Point | Status |
|---|-------|--------|
| 1 | Drop-in C header and shared library, all public functions working | done on Linux |
| 2 | Graceful handling of out-of-memory situations | done while parsing |
| 3 | Protection against billion laughs, parameter laughs and quadratic runtime, without rejecting benign documents | done, as libexpat |
| 4 | External entities available, disabled by default | done, as libexpat |
| 5 | Security beyond memory safety | partial |
| 6 | XML 1.0 fifth edition rather than fourth edition or 1.1 | open |
| 7 | Full support for DTD, parameter entities, general entities | done, as libexpat |
| 8 | Testing against the XML specification and libexpat's test suite | done |
| 9 | Parsing speed above libexpat compiled with Fil-C | done on one machine |
| 10 | Continuous bug and feature parity with libexpat | partial |
| 11 | Sharing bugs found in libexpat with upstream | open |
| 12 | Maintaining the rewrite | ongoing |
| 13 | MIT or similar licence | done |

## 1. Drop-in C interface

Done: `crates/expat-capi` exports every function declared in libexpat
2.10.0's `expat.h` (D-047 to D-057), built as a static and a shared
library. A C trace program built once against libexpat and once against
`expat-capi` gives equal output on the upstream vectors under every
handler set (`scripts/capi-check.sh`, 152,610 runs at D-057).

`scripts/build-dropin.sh` builds it as `libexpat.so.1` with libexpat's
headers, and libexpat's own test suite passes against that library except
for 12 tests of libexpat internals (`scripts/runtests-check.sh`, D-060).
`XML_ParserCreate_MM` receives every allocator call libexpat makes.

Remaining:
- Library names and symbol versioning for macOS and Windows.
- After `XML_ParserReset`, keep tags, pools and tables as libexpat does
  (bindings are kept), so allocator calls after a reset match too.
- Known differences are documented: `XML_GetIdAttributeIndex` before the
  first start tag (D-056), converter call counts of unknown encodings
  (D-057).

## 2. Out of memory

Done: every allocation point of the parser is failed in turn and compared
with libexpat through `XML_ParserCreate_MM` (D-058, 83,023 failure points,
equal results).

Done as well: while parsing, the port's collections grow fallibly
(`try_reserve`), so a failure of the system allocator ends the parse with
`XML_ERROR_NO_MEMORY` instead of aborting; a test with a failing global
allocator fails every allocation in turn and checks that the process
does not abort (D-061).

Remaining: creating, resetting and configuring a parser (`Parser::new`,
`reset`, `XML_SetEncoding`) still abort on allocation failure, as does the
reference-count block of an internal entity's shared text (stable Rust
has no fallible `Arc` constructor), and the input buffer's reference
count when `XML_GetBuffer` is called inside a handler.

## 3. Amplification and quadratic runtime

Done: libexpat's billion-laughs accounting and allocation tracker are
ported with exact parity (D-034, D-046); parameter entities are counted as
in libexpat. The namespaced-attribute duplicate check was quadratic in an
earlier version of the port and is fixed, with a scaling test against C on
three platforms (REPORT, "Part 2"). Limits and measured peaks:
`docs/DTD-SECURITY.md`.

Remaining: measure benign large DocBook and DITA documents against the
default limits, and add them to the benchmark corpus.

## 4. External entities

Done, as libexpat: the library never fetches anything; an external entity
reaches the application's handler only, and parameter entity parsing is
off by default (D-055).

## 5. Security beyond memory safety

Done: the parser crate has `#![forbid(unsafe_code)]`; unsafe code is
limited to the C interface and documented per block. CI runs Miri, fuzz
smoke tests, `cargo deny`, and a check of the vendored C sources. The
CVEs fixed up to libexpat 2.10.0 are checked against the port
(`tests/tests/cve_2_10_0.rs`).

Remaining: OSS-Fuzz integration; a security policy for coordinated
disclosure together with libexpat for issues that affect both.

## 6. XML 1.0 fifth edition

Open. The port follows libexpat's name rules (fourth edition); in the W3C
suite 306 fifth-edition name-character tests are rejected by both
(D-059). Changing this departs from libexpat, so it needs a decision with
upstream: either both move to fifth edition, or the port offers it as an
option outside the parity tests.

## 7. DTD and entities

Done, as libexpat (non-validating): internal and external subsets,
parameter and general entities, attribute defaults, the declaration
handlers (D-053).

## 8. Testing against the specification and libexpat's tests

Done: the 540 inputs taken from libexpat's unit tests are compared under many
option sets (`tests/vectors/upstream.jsonl`); the W3C XML Conformance Test
Suite is compared (D-059, 2,586 documents, 12,930 runs, equal); fuzzing
compares both parsers on generated input.

Done as well: libexpat's own test suite against the drop-in library
(D-060, see 1).

## 9. Speed

Done on one aarch64 Linux VM: 1.91x-2.44x the throughput of libexpat
built with Fil-C 0.687 (2026-11-13), and after D-043 to D-046 0.68x-1.24x
of libexpat built with clang `-O2` (median 1.00x). REPORT, "Benchmarks".

Remaining: x86_64 figures; Fil-C figures after D-043 to D-046; UTF-16
input, the slowest case.

## 10. Parity over time

Done: the port is pinned to libexpat 2.10.0 and compared with the vendored
copy on every CI run.

Remaining: a written procedure for each libexpat release (update the
vendored copy, run the comparison, port the changes, record them in
`CHANGELOG.md`), and a CI job that compares with libexpat's development
branch.

## 11. Upstream

Open, not planned at present. Two findings could be reported to libexpat:
- `m_idAttIndex` is read before it is first set, and stays stale after
  `XML_ParserReset` (D-056).
- Undefined behaviour in the accounting of a name cut off by the end of
  the buffer in the epilog (Q-4, D-029, `docs/DIVERGENCES.md`).

## 12. Maintenance

Ongoing. Decisions are recorded in `docs/DECISIONS.md`, divergences in
`docs/DIVERGENCES.md`. Contributors and reviewers are welcome
(`CONTRIBUTING.md`).

## 13. Licence

MIT, as libexpat.

## Order of work

1. Release procedure and a comparison with libexpat's development branch
   (10).
2. x86_64 and Fil-C benchmarks (9); DocBook and DITA documents (3).
3. Fifth edition names, after discussion with upstream (6).
