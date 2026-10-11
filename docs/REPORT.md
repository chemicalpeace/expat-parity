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
`xmlparse.c` with C's allocation behaviour; module 10 is the parser core of
`xmlparse.c` for documents without a document type declaration (buffer,
prolog, content, attributes, namespaces, CDATA, epilog, positions). No
public parser API exists yet (module 12); the DTD, entities and the
amplification limits are module 11.

## Verification infrastructure

- Oracle: vendored libexpat, byte-identical to the pinned commit
  (`scripts/verify-vendor.sh`, 39 files).
- Trace format = definition of "equal" (`docs/ARCHITECTURE.md`).
- 540 upstream-derived vectors (`tests/vectors/`), golden traces, 7 upstream
  error expectations asserted, 7 option sets in the differential matrix
  (10 for whole parses since module 10).

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

  | `parse_trace` | module 10: whole parses, port vs C by trace (documents with a DTD skipped), options from two selector bytes | 672 s (7 slices of 96 s) | 3,486,658 |
  | `buffer_diff` | module 10: `XML_Parse`/`XML_GetBuffer`/`XML_ParseBuffer` programs, buffer state after every operation, white-box | 672 s (7 slices of 96 s) | 5,605,579 |
  | `parse_trace` | module 10, final target (namespace-triplet selector bit, triplet seeds), started from the re-merged corpus | 673 s (7 slices of 96-97 s) | 2,444,032 |

  Module 10 figures are the sums of `stat::number_of_executed_units` in the
  libFuzzer logs kept in `docs/fuzz-logs/module-10/` (one file per slice;
  `-max_len=65536 -rss_limit_mb=2048`, ASan). `parse_trace`: edge coverage
  (`cov:`) 9,282 at the start of the first slice to 9,390 at the end of the
  fifth (features 37,102 to 38,633); slices 6 and 7 restarted from the
  minimised corpus because a sandbox reset deleted the working corpus
  (9,282 to 9,324). `buffer_diff`: 5,746 to 6,358 edges (features 19,535 to
  23,376), still growing slowly. The instrumented binaries contain 54,668
  counters each, including the C oracle and the harness, so the edge counts
  are not a coverage percentage of the port. The final `parse_trace` run
  (logs `parse_trace-*-f1800cad.log` and `-072e1bb8.log`; the port's code is
  the same in both commits) went from 10,277 to 10,647 edges (features
  40,969 to 43,939). Findings: none in these runs.
  Earlier module 10 runs found the `errorProcessor` bug (D-023, fixed in
  89c7e9e). Figures reported before for module 10 (`parse_trace` 665 s /
  5,223,814, `buffer_diff` 790 s / 6,563,968) came from a summary file of
  one line per slice, without the raw logs; two `buffer_diff` lines were
  recorded from the console because their slices ran concurrently with
  `parse_trace` and lost their log lines, and two of those were entered
  twice. They are not used; the runs above replace them.

  Seeding `buffer_diff` with the upstream vectors for the corpus merge
  showed a gap in the target: it compared documents with a DTD although
  the port stops at them (136 inputs failed, every one with the port's
  `Stop::DtdNotPorted` code). The fuzzer had never produced such an input.
  The target now skips them as `parse_trace` does
  (`expat_difftest::fuzzcase::buffer_diff_case`); module 11 removes both
  skips.

  Corpora were minimised with `cargo fuzz cmin` and committed
  (`fuzz/corpus/`). `core_diff` was run in 95-second slices because a
  command may run at most two minutes in the current sandbox; two earlier
  slices (216 s, 65,802 executions, no finding) are not counted because a
  deliberate-bug experiment edited the source while they were being built.
  Since module 10 the corpora, the seed generator and the merge command are
  documented in `docs/FUZZING.md`; `tests/tests/fuzz_corpus.rs` replays the
  `parse_trace` (9,238 files) and `buffer_diff` (6,787 files) corpora on
  every CI platform.

### Module 10 (PR #5): the parser core versus C

**What "equal" means.** Each parse produces a canonical trace
(`docs/ARCHITECTURE.md`, "Trace format"): one line per callback (XML
declaration, start tag with its specified-attribute count and every
attribute in libexpat's order, end tag, character data coalesced into one
line between other events, PIs, comments, CDATA start/end, namespace
start/end, skipped entities, and the DTD events once ported), ending with
`result <ok|error> <error_code> <line> <column> <byte_index>` taken from
`XML_GetErrorCode` and the position getters after the last call. C's trace
comes from the vendored libexpat 2.9.0 through `shim/oracle_trace.c`; the
port's from `tests/src/port.rs`. Two traces must be byte-identical; the only
normalisation is the coalescing of character data (libexpat splits it at
buffer boundaries, which applications cannot rely on).

**The 398 upstream vectors under 11 option sets.**
`tests/tests/parse_diff.rs::upstream_vectors_match_c` parses each of the
540 documents extracted from libexpat's test suite (`tests/vectors/`) with
both parsers under each of these option sets:

| # | Option set |
|---|---|
| 1 | defaults: no namespaces, no encoding override, whole document in one `XML_Parse` call |
| 2 | 1-byte chunks (`XML_Parse` per byte, last call final) |
| 3 | 7-byte chunks |
| 4 | namespaces, separator `\|` |
| 5 | encoding override `ISO-8859-1` |
| 6 | encoding override `UTF-16` |
| 7 | parameter entity parsing `ALWAYS`, namespaces with separator `\n`, 3-byte chunks |
| 8 | 3-byte chunks through `XML_GetBuffer` + `XML_ParseBuffer` |
| 9 | 1-byte chunks, reparse deferral off, namespaces with separator `:` |
| 10 | 2-byte chunks through the buffer API, encoding override `UTF-16LE` |
| 11 | namespaces with separator `\|` and `XML_SetReturnNSTriplet`, 5-byte chunks |

Chunking means the document is fed in pieces of that many bytes, each a
separate `XML_Parse` (or `XML_GetBuffer` with a request of `2n + 3` bytes
then `XML_ParseBuffer(n)`), with `isFinal` on the last; this exercises
partial tokens, the reparse-deferral heuristic and buffer growth. 398 of
the 540 vectors are compared under all 11 option sets (4,378 trace pairs)
and every pair is equal; with the pairs of DTD vectors whose parse ends
before the DTD the test compares 4,670 pairs and counts 1,270 stopped
ones. The other 142 contain a document type declaration;
the port stops at it with `Stop::DtdNotPorted` and those pairs are counted,
not compared (module 11). The test asserts the count (`fully == 398`), so a
change in scope is visible. Generated documents add a second population:
`generated_documents_match_c` parses 2,500 seeded documents
(`expat_difftest::docgen`: XML declarations, nesting, attributes with and
without namespaces, references, CDATA, PIs, comments, encoded as UTF-8,
UTF-16LE/BE with and without BOM or Latin-1, a third of them damaged by
truncation or byte edits) under 3 random option sets each (14 encoding
overrides including an unknown one, 6 separators, 10 chunk sizes, buffer
API, reparse deferral, namespace triplets): 7,500 pairs, all equal.
`namespace_triplets_match_c` adds 7 hand-written namespace documents x 3
separators x 3 chunkings x triplets on/off. The buffer
itself is compared white-box (`tests/tests/buffer_diff.rs`: buffer
offsets, limit, `m_lastBufferRequestSize`, parse-end and event pointers
after every operation).

Local run (2026-10-11, aarch64 Linux):
`cargo test -p expat-difftest --test parse_diff -- --nocapture` →
`upstream: 398 of 540 vectors compared under every option set`,
`upstream: 4670 pairs equal to C, 1270 stopped at a DTD (module 11)`,
`generated: 7500 pairs equal to C`, 5 tests passed.

**Injected bugs** (module 10, 2026-10-11, each reverted after the run):

- `XML_CONTEXT_BYTES` 1023 instead of 1024 (`parse/mod.rs`):
  `buffer_diff::buffer_management_matches_c_on_generated_sequences` and
  `large_documents_cross_buffer_boundaries_like_c` failed.
- Not setting the error position before `XML_ERROR_DUPLICATE_ATTRIBUTE`
  (`parse/content.rs`): `parse_diff::generated_documents_match_c` (271
  mismatches), `buffer_diff::buffer_management_matches_c_on_generated_sequences`,
  and the corpus replays (`parse_trace` 23, `buffer_diff` 17 failing inputs)
  failed; `upstream_vectors_match_c` did not.
- Plain equality instead of C's prefix comparison in the duplicate check
  for prefixed attributes (`parse/content.rs`): **not detected** by any
  test at first. Root cause: the harness never enabled
  `XML_SetReturnNSTriplet`, and the two comparisons only differ when the
  stored name carries the triplet suffix. Fixed by adding the
  `ns_triplets` option to the oracle and the port adapter (option set 11,
  generated documents, the `parse_trace` selector, seeds) and
  `namespace_triplets_match_c`, which then failed on the mutation
  (`C nsend b`, port `start e 4` for
  `<e xmlns:a='u' xmlns:b='u' a:x='1' b:x='2'/>`).

The earlier fuzzing found one real port bug: the `errorProcessor` reset
(D-023), fixed in 89c7e9e with a unit test.

**Coverage** of `crates/expat` (the port), measured with
`cargo-llvm-cov 0.8` (`cargo llvm-cov --no-report` per test target, then
`cargo llvm-cov report --ignore-filename-regex
'(tests/|crates/expat-oracle|benches/|fuzz/|vendor)'`), toolchain 1.99.0,
commit 6504844 (the module 10 code; the later commits of PR #5 add
the namespace-triplet tests and change no port code):

| Metric | Covered | Total | % |
|---|---|---|---|
| Regions | 7,910 | 8,418 | 93.97 |
| Functions | 395 | 428 | 92.29 |
| Lines | 4,397 | 4,749 | 92.59 |

Per module (lines): `parse/content.rs` 82.18 %, `parse/mod.rs` 82.78 %,
`parse/prolog.rs` 90.54 %, `parse/dtd.rs` 95.76 %, `role` 99.44 %,
`tok/scan.rs` 97.64 %, `pool` 97.55 %, `hash` 96.02 %, `chartab` 48.57 %
(the runtime table accessors; the tables are checked byte for byte against
C, mostly by generated-table checks). Included: the unit tests of every
crate and the integration tests `parse_diff`, `buffer_diff`,
`upstream_vectors`, `trace_format`, `harness_selftest`, `module_diff`,
`core_diff`, `role_diff`, `init_diff` and 11 of the 12 `tok_diff` tests.
Not included: doctests (need nightly `--doctests`), and
`tok_diff::upstream_vectors_every_offset`, which does not finish within the
sandbox's two-minute command limit in an unoptimised instrumented build
(it exercises only the tokenizer, already at 97-100 % lines). Fuzzing
coverage is not included. Uncovered parser code is mostly the
suspend/resume and handler-status branches that only the public API
(module 12) can reach (D-023) and allocation-failure paths.

### Module 11a (declarations): the DTD versus C

Specification first: `docs/DTD-SECURITY.md` (C 2.9.0 entity behaviour,
external access, recursion, amplification and allocation limits, error
codes, with `xmlparse.c` line references) and D-026 (security defaults
identical to C), committed before the port (a651129).

Ported in 11a (`parse/decl.rs`, `parse/model.rs`, `parse/dtd.rs`): every
`doProlog` role of the internal subset — doctype name/public/system id,
element declarations with content models (`nextScaffoldPart`,
`build_model`, connector checks), attribute-list declarations (types,
enumerations, `NOTATION`, defaults normalised with `storeAttributeValue`,
`defineAttribute`), general and parameter entity declarations with
`storeEntityValue`, unparsed entities, notations, parameter entity
references when they are not expanded, and in content and attribute
values every entity check that does not expand: undeclared, declared in a
PE, recursive, unparsed (`XML_ERROR_BINARY_ENTITY_REF`), external in an
attribute (`XML_ERROR_ATTRIBUTE_EXTERNAL_ENTITY_REF`), external in content
(skipped without a handler). Still stopping with `Stop::DtdNotPorted`:
expanding a declared internal entity (content, attribute value, parameter
entity in the prolog) — 11b — and the external subset / conditional
sections, which need external entity parsers (module 12).

Local run (2026-10-12, aarch64 Linux, commit ae1413a):
`cargo test -p expat-difftest --test parse_diff -- --nocapture` →
`upstream: 494 of 540 vectors compared under every option set` (was 398),
`upstream: 5634 pairs equal to C, 306 stopped at a DTD`,
`generated dtd: 8938 pairs equal to C, 62 stopped at a DTD` (3,000
seeded documents with a DTD x 3 random option sets including all three
parameter-entity-parsing modes, a quarter damaged),
`attribute definitions: 88 pairs equal to C`, `deep models` (content
models nested 1, 31, 32, 33, 1,000 and 20,000 groups, whole and in
1,000-byte chunks) equal, `generated: 7500 pairs equal to C`; 9 tests
passed, no mismatch. `generated_dtd_documents_cover_outcomes` asserts that
the generator reaches the `doctype`, `enddoctype`, `entity`, `notation`,
`element`, `attlist` and `skipped` events and the error codes 0, 2, 4, 10,
11, 14, 15, 16 and 32 (measured: `{0: 562, 2: 277, 3: 30, 4: 783, 5: 86,
8: 68, 10: 32, 11: 461, 12: 2, 14: 20, 15: 6, 16: 7, 17: 6, 18: 245,
19: 48, 27: 87, 28: 36, 30: 35, 32: 207, 40: 2}` over 3,000 documents).
Every compared pair includes the error code, line, column and byte index.

**Injected bugs** (2026-10-12, each reverted after the run):

- `normalizePublicId` without removing the trailing space
  (`parse/decl.rs`): `generated_dtd_documents_match_c` failed
  (`C doctype a doc.d spaced%20id 1`, port `... spaced%20id%20 1`).
- Last instead of first attribute definition recorded in
  `defaultAttForName` (`parse/dtd.rs`): **not detected** at first. Root
  cause: `storeAtts` in the port looked the type up in the default list
  instead of through `defaultAttForName`, so the map was not used the way
  C uses it. Fixed by using the map like C and adding
  `repeated_attribute_definitions_match_c`, which then failed on the
  mutation (`C attr x %20%20p%20%20q%20%20`, port `attr x p%20q` for
  `<!ATTLIST a x CDATA #IMPLIED x NMTOKEN #IMPLIED>`).

**Fuzzing** (aarch64 Linux, `cargo-fuzz` 0.13, AddressSanitizer, flags
`-max_len=65536 -rss_limit_mb=2048`, commit ae1413a, working corpora
started from the committed corpora plus the new seeds, which add 2,000
DTD documents to `parse_trace` and 1,000 to `buffer_diff`), from the
trimmed logs in `docs/fuzz-logs/module-11a/`:

| Target | Slices | Seconds (sum of `Done ... in N second(s)`) | Executions (sum of `stat::number_of_executed_units`) | Findings |
|---|---|---|---|---|
| `parse_trace` | 7 | 682 | 2,223,813 | none |
| `buffer_diff` | 7 | 689 | 5,237,959 | none |

Three further slices of each target were started concurrently by mistake
and wrote into one log file each; those logs are mixed and are not counted
(two of the concurrent `buffer_diff` processes exited with status 1 and
no crash artifact, consistent with corpus files removed under a running
process). To rule out a hidden finding, every input of both final
working corpora was then executed once (`-runs=0`): 24,348 `parse_trace`
and 22,392 `buffer_diff` inputs, exit status 0. The merged corpora
(`-merge=1` of committed corpus, working corpus and seeds) are committed:
`parse_trace` 10,647 files (41,905 inputs, 13,750 edges), `buffer_diff`
9,674 files (32,718 inputs, 12,256 edges); both are replayed by
`tests/tests/fuzz_corpus.rs` on every CI platform.

## Divergences

`docs/DIVERGENCES.md`. No divergences yet. Three C quirks found by the
harness, all chunking-related: Q-3 resolved as "replicate" (module 7; the
input violates XML 1.0 §4.3.3), Q-1 and Q-2 resolved as "replicate"
(module 10, D-024).
Platform-dependent C behaviour fixed to one choice: D-016 (counter width),
D-017 (sign of stray bytes in `utf8_toUtf16`).

## Benchmarks

Only an oracle baseline exists (`cargo bench -p expat-bench`); port-vs-C
comparisons start when the parser exists.

## Limitations

- No public parser API yet (module 12): no default handler,
  `XML_StopParser`/`XML_ResumeParser`, external entity parsers,
  unknown-encoding handler or `XML_GetInputContext`.
- After module 11a the port stops with `Stop::DtdNotPorted` (never a
  guessed error) only when a declared internal entity is expanded
  (module 11b) or where an external entity parser would be needed
  (module 12); 46 of the 540 upstream vectors are not yet compared under
  every option set.
- The allocation tracker's byte counts (`XML_SetAllocTrackerMaximumAmplification`)
  are replicated for the hash tables only; whether every parser structure is
  replicated is decided in module 11 (D-021, D-023). Without a failing
  allocator or a tracker limit nothing observable depends on it.
- D-017: on targets where C `char` is unsigned (aarch64 Linux) C's
  `utf8_toUtf16` zero-extends stray bytes that the port sign-extends; the
  parser never passes such bytes to the converter, and the tokenizer
  harness accepts exactly those units there. Every CI target has a signed
  `char`.
- Q-1/Q-2 (DIVERGENCES.md): positions after a CR at the end of the input
  depend on chunking in C; the port replicates it.
- `XML_UNICODE` builds are out of scope (D-002).

## Reproduce

```sh
cargo test --workspace --all-features
scripts/ci-local.sh
cargo run -p expat-difftest --bin oracle_trace -- some.xml
cargo run -p expat-difftest --bin fuzz_seeds -- /tmp/seeds   # docs/FUZZING.md
```
