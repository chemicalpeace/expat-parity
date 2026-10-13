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
public parser API exists yet (module 12); module 11 (in progress) is the
DTD: 11a declarations, 11b internal entity expansion and recursion, 11b2
billion-laughs limits and 11c the allocation tracker are done; external
entities (11d) are next.

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

### Module 11b (internal entity expansion and recursion): entities versus C

Ported: `processEntity`, `internalEntityProcessor` and the three open-entity
lists of `xmlparse.c` (content / parameter entities, attribute values,
entity values; `parse/entity.rs`), the event pointers of open entities
(`internalEventPtr`/`internalEventEndPtr`, D-028), expansion of internal
general entities in content, attribute values and default attribute values,
of internal parameter entities between declarations and inside entity
values, and every recursion and nesting check of those paths. With this,
all 540 upstream vectors are compared under every option set.
`Stop::DtdNotPorted` remains only where C would call an external entity
reference handler with an external entity parser (external subset,
external parameter entities with a handler) — module 11d / 12.

Local run (2026-10-12, aarch64 Linux, commit 263d94d, `--release`):
`cargo test -p expat-difftest --test parse_diff -- --nocapture` →
`upstream: 540 of 540 vectors compared under every option set`,
`upstream: 5940 pairs equal to C, 0 stopped at a DTD`,
`generated dtd: 9000 pairs equal to C, 0 stopped at a DTD`,
`generated: 7500 pairs equal to C`; and
`cargo test -p expat-difftest --test entity_diff -- --nocapture` →
`curated entity cases: 1014 pairs compared, 0 mismatches` (78 documents x
13 option sets: the pinned `ALWAYS` run plus every parameter-entity-parsing
mode whole and chunked at 1 and 7 bytes, and with namespaces through the
buffer API in 3-byte pieces),
`deep chains: 30 pairs compared, 0 mismatches`,
`deep cycles: 3 pairs compared, 0 mismatches`,
`generated entity documents: 6000 pairs compared, 0 mismatches` (result
codes reached: `{0: 1317, 2: 203, 3: 45, 4: 2156, 5: 277, 6: 3, 11: 363,
12: 510, 13: 54, 14: 919, 15: 64, 16: 38, 18: 1, 30: 50}`; the test
asserts that 0, 4, 11, 12 and 13 are reached). Every compared pair includes
the error code, line, column and byte index.

**Test groups** (`tests/tests/entity_diff.rs`; each curated case pins
C's error code under `XML_PARAM_ENTITY_PARSING_ALWAYS`, the port is
compared under every option set):

| Group | Cases | C behaviour (pinned codes) | Security policy | Known divergences |
|---|---|---|---|---|
| Content (`content-*`) | 20 | Replacement text is tokenised as content in the internal UTF-8 encoding and reported as if inline: markup, comments, PIs, CDATA, character and predefined references, CR normalisation, nested entities; first declaration wins. Errors: undeclared entity in replacement text 11, unparsed entity 15, bad markup 4/5/7, bad character reference 14. Error positions are those of the outermost reference in the document (DTD-SECURITY.md section 3). | Same as C (D-026): expansion is on, bounded only by the amplification limit (step B). | none |
| Attribute values (`attr-*`) | 13 | Replacement text is normalised (`appendAttributeValue`); `<` in it is 4, undeclared 11, external entity 16, unparsed 15, bad character reference 14, duplicate attribute after expansion 8. | Same as C; external entities are never read in attribute values. | none |
| Default attribute values (`default-*`) | 8 | Normalised once at declaration time (`XML_ACCOUNT_NONE`), applied by `storeAtts`; undeclared reference 11. | Same as C. | none |
| Parameter entities (`pe-*`) | 18 | Expanded between declarations and inside entity values of a PE only when parameter entity parsing is not `NEVER`; in an internal-subset entity value a PE reference is 10; a PE ending inside markup 29; an entity declared in a PE referenced from a standalone document 24; bad markup 4/5; undeclared 11. | Default `NEVER` like C (D-026); no external PE is read without a handler (none exists before step D). | none |
| Recursion (`rec-*`) | 15 | Direct and indirect recursion in content, attribute values, default values, PEs and entity values: 12 at the reference that closes the cycle, when it is expanded (declaration alone is no error); one case where the self-reference is never expanded is 0. | Same as C. | none |
| Nesting (`async-*`) | 4 | An element or PE opened in an entity must close in it: 13. | Same as C. | none |
| Deep chains | 5 contexts x depth 1/100/10,000 x 2 | Accepted by C; expansion is iterative in C (2.7.0+) and in the port, so 10,000 open entities cost heap, not stack. | Inputs < 600 KB, expansion < 50 KB. | none |
| Deep cycles | 3 | A cycle closed after 5,000 entities (content, attribute, PE): 12. | Bounded as above. | none |
| Generated entity graphs | 6,000 | Random declarations/references over all contexts, a fraction damaged, random option set. | Seeded generator (deterministic). | none |

**Injected bugs** (2026-10-12, each reverted after the run; output in the
format of `cargo test -p expat-difftest --test entity_diff --test parse_diff`):

| # | Mutation | Detected by |
|---|---|---|
| I1 | Recursion check removed in content (`doContent`, `content.rs`) | `deep_recursion_cycle_matches_c`: `cycle-content` C `result error 12 1 122798 122798`, port `result error 43 ...` (the port ran into the amplification limit instead); 104 generated mismatches |
| I2 | `XML_ERROR_ASYNC_ENTITY` check when an entity completes removed (`internal_entity_processor`) | `generated_entity_documents_match_c`: 2 mismatches (`gen-296` C `result error 13 1 262 524`, port `result error 7 ...`). The curated `async-*` cases hit the other check sites only; the generator covers this one. |
| I3 | `entity->open` not cleared when an entity is closed | 100 curated mismatches (`content-markup` C `start a 2`, port `result error 12 1 82 82`) |
| I4 | Event pointer of the open entity ignored (document pointer moved inside expansions) | 254 curated mismatches (`content-undeclared-in-entity` C `result error 11 1 35 35`, port `result error 11 1 0 0`) |
| I5 | PE reference in an internal-subset entity value accepted (`XML_ERROR_PARAM_ENTITY_REF` dropped) | 26 curated mismatches (`pe-nested` C `entity p 1  ~ ~ ~ ~`, port `entity p 1 %25q; ~ ~ ~ ~`) |
| I6 | Recursion check removed in attribute values (`appendAttributeValue`) | `cycle-attr` C `result error 12 ...`, port `result error 43 ...`; 65 curated mismatches (`rec-attr-direct`) |

(An earlier set of injected bugs run during development was not recorded
with its output; the table above was re-run and is the one reported.)

**Fuzzing** (aarch64 Linux, `cargo-fuzz` 0.13, AddressSanitizer, flags
`-max_len=65536 -rss_limit_mb=2048`; one slice at commit e077074e, the
rest at 263d94d; strictly sequential slices under a lock; working corpora
started from the committed corpora plus the entity seeds, 1,500 for
`parse_trace` and 500 for `buffer_diff`), from the trimmed logs in
`docs/fuzz-logs/module-11b/`:

| Target | Slices | Seconds (sum of `Done ... in N second(s)`) | Executions (sum of `stat::number_of_executed_units`) | Findings |
|---|---|---|---|---|
| `parse_trace` | 7 | 673 | 2,099,284 | none |
| `buffer_diff` | 7 | 672 | 4,836,238 | none |

Merged corpora (`-merge=1` of committed corpus, working corpus and seeds,
`docs/FUZZING.md`): `parse_trace` 36,840 inputs to 11,036 files (14,108
edges), `buffer_diff` 29,229 inputs to 10,208 files (12,551 edges); both
scanned for personal data before commit (no hits) and replayed by
`tests/tests/fuzz_corpus.rs` on every CI platform.

### Module 11b2 (billion-laughs limits): amplification accounting versus C

Ported: `XML_SetBillionLaughsAttackProtectionMaximumAmplification` and
`...ActivationThreshold` (`Parser::set_billion_laughs_*`, D-029); the
accounting itself (`parse/accounting.rs`) existed since module 10 and is now
exercised and compared counter by counter. The oracle gained the four limit
setters, `trace_accounting`, `text_digest` and a capped allocator; the
harness gained bounded comparisons (`expat_difftest::bounded`).

Local run (2026-10-12, aarch64 Linux, commit 1eb3a5f, `--release`):
`cargo test -p expat-difftest --test amplification_diff -- --nocapture`:

| Group (test) | Pairs | C behaviour | Security policy | Known divergences |
|---|---|---|---|---|
| Setters (`setters_match_c`) | 84 | 14 factors x 6 thresholds: NaN, -NaN, -inf, -1, ±0, 0.5, 0.9999999 refused (`limit bl-amplification 0`); 1.0, 1.0000001, 1.5, 100, `f32::MAX`, +inf accepted; every threshold (0 ... `u64::MAX`) accepted. | Same as C; the port never relaxes a refused value. Non-root refusal: needs external entity parsers (11d). | none |
| Default limits (`default_limits_match_c`) | 26 | Billion laughs (10 levels x 10) in content, attribute value, default value, parameter entity, entity value: 43 after just over 8 MiB, under every feed (whole, 1, 7, 4096-byte buffer API with namespaces); 6 levels (3 MB): accepted; 195 KB direct + 9.2 MB expansion (factor ~48): accepted. | C's defaults (100.0, 8 MiB), D-026. | none |
| Custom limits (`custom_limits_on_generated_documents_match_c`) | 4,000 | Generated entity documents under factors 1.0-100 and thresholds 0-8 MiB, random feed: codes `{0: 832, 2: 127, 3: 24, 4: 1346, 5: 158, 11: 222, 12: 315, 13: 55, 14: 646, 15: 26, 16: 22, 18: 2, 30: 47, 43: 178}`. | Same as C. | none |
| Threshold sweep (`threshold_sweep_matches_c`) | 987 | Factor 1.0, every threshold 0 ... total output + 2, whole and 3-byte chunks, five contexts: 43 at exactly the token where `direct + indirect >= threshold`. | Same as C. | none |
| Amplification sweep (`amplification_sweep_matches_c`) | 13,689 | Threshold 0, every factor `(d + i) / d` the counters can take (computed in `float`) and its float neighbours: the `<=` boundary is hit exactly. | Same as C. | none |
| Quadratic blow-up (`quadratic_blowup_matches_c`) | 10 | A 16 KiB entity referenced 1,024 times in content and in one attribute value: 43 by default under every feed; accepted with factor 1,000. | Same as C; no extra cap on replacement text (D-026). | none |
| Breach positions (`breach_positions_match_c`) | 80 | 4-level documents over 6 lines, four small limit pairs, every feed: 60 pairs end with 43; line, column, byte index and counters equal. | Same as C. | none |
| Q-4 (`q4_epilog_name_at_end_of_buffer`) | 3 | `<r/>junk`: C error 9 with an address-dependent direct counter (`NULL - s`, undefined behaviour). | Port counts 0 bytes for the rejected token. | Q-4: counter only, masked (DIVERGENCES.md) |

All groups: 0 mismatches. Every pair compares events, error code, line,
column, byte index and both accounting counters.

**Bounds** (DTD-SECURITY.md, "Bounds of the resource-limit tests"): input
16 KiB / 256 KiB, wall clock 20 s / 60 s per pair, heap 64 MiB / 256 MiB per
side. Measured: largest peak 39.9 MB (port) / 33.6 MB (C) in the quadratic
attribute case, 8.4 MB / 4.2 MB for default-limit billion laughs, slowest
pair 0.42 s, whole file 6.4 s.

**Injected bugs** (2026-10-12, each reverted after the run, `amplification_diff`):

| # | Mutation | Detected by |
|---|---|---|
| J1 | Activation test off by one (`output < threshold` became `<=`) | threshold sweep, 96 mismatches (`thr-content-63`) |
| J2 | Amplification test off by one (`<=` became `<`) | breach positions, 24 mismatches (`pos-content-1-0`, factor 1.0: the port stops with 43 at the first token, whose factor equals the limit; C reports `xmldecl` and goes on) |
| J3 | Activation threshold ignored | custom limits 93, quadratic 8, setters 6, threshold sweep 256 mismatches |
| J4 | Factor setter accepts values below 1.0 | setters, 36 mismatches |
| J5 | Expansion bytes counted as direct | breach positions, 80 mismatches |
| J6 | Threshold setter ignored | breach positions, 60 mismatches (C `text %0A%20%20` + 43, port expands on) |

**Fuzzing** (`parse_trace` only: the only target whose code path changed —
the oracle trace shim, the port's setters and the selector; `buffer_diff`
is unchanged). The second selector byte's bit 7 now sets small limits with
the counters traced. Working corpus: committed corpus (11,036) + 500 new
seeds with small limits. From the trimmed logs in
`docs/fuzz-logs/module-11b2/` (commit ffb5a5a, strictly sequential slices):

| Target | Slices | Seconds | Executions | Findings |
|---|---|---|---|---|
| `parse_trace` | 7 | 673 | 2,062,998 | none (coverage 14,108 -> 14,344 edges) |

The corpus replay before fuzzing found Q-4 (seven committed inputs whose
selector already had bit 7 set). Merged corpus: 36,002 inputs to 11,174
files (14,355 edges), scanned for personal data (no hits).

### Module 11c (allocation tracker): the tracker relation versus C

Ported: `MALLOC_TRACKER` and its setters (`parse::tracker`,
`Parser::set_alloc_tracker_*`). The port charges its tracker at C's
allocation points or earlier, at least C's bytes (D-030); the input buffer
is not charged, as in C (`xmlparse.c` 2601-2603). The oracle gained a
tracker readout (bytes after creation and at the end of the parse).
Because the counts are not byte-identical, the tests check the relation
that matters for security (`expat_difftest::tracker`): **whenever C stops
with `XML_ERROR_NO_MEMORY`, the port does too**; at the default limits the
port never refuses a document C accepts; every other trace line is equal.
Each `check` runs a document three times: under the group's options, then
with factor 1.0 and the threshold at C's final count, and at half of it.

Local run (2026-10-12, aarch64 Linux, commit 929fcb7, `--release`):
`cargo test -p expat-difftest --test tracker_diff -- --nocapture`:

| Group (test) | Runs | C behaviour | Security policy | Known divergences |
|---|---|---|---|---|
| Setters (`setters_match_c`) | 9 | Factor NaN, -1, 0, 0.999999 refused; 1.0 and +inf accepted; thresholds 0 and `u64::MAX` accepted; factor 1.0 with threshold 0 fails at the first tracked allocation (error 1). | Same validation as C; defaults 100.0 / 64 MiB unchanged (D-026). Non-root refusal: needs external entity parsers (11d / 12). | none |
| Creation charge (`creation_charge_covers_c`) | 8 | 3,040 bytes after `XML_ParserCreate(NS)`, 3,054-3,059 with an encoding name (UTF-8, ISO-8859-1, UTF-16LE). | Port charges 16,384 (+ name), an upper bound. | DV-1 margin |
| Upstream vectors (`upstream_vectors_tracker_relation`) | 6,480 (540 vectors x 4 option sets x 3) | C refuses in 3,676 tight runs. | Port refuses in all of them; 4 tight runs refused by the port only; port total >= C's at the end (smallest margin 12,272 bytes). | DV-1 (4 tight runs) |
| Generated documents (`generated_documents_tracker_relation`) | 4,500 (1,500 documents x 3) | C refuses in 2,798 tight runs. | Port refuses in all of them; 20 refused by the port only; smallest margin 11,764 bytes. | DV-1 (20 tight runs) |
| Threshold sweep (`threshold_boundary_sweep`) | 9 documents x 4 option sets: 50 grid thresholds (1 KiB-64 MiB) + C's boundary found by bisection, probed at -64, -1, 0 | 108 boundary probes where C refuses. | Port refuses in all 108; at C's exact passing threshold the port refuses in 36/36 (asserted). | DV-1 (36/36) |
| Amplification sweep (`amplification_boundary_sweep`) | 1,152 (36 x 32 factors 1.0-4,000, threshold 0) | C refuses in 986 probes. | Port refuses in all of them. | DV-1 |
| Tracker-only attribute (`tracker_only_attribute`) | 3 | 85,000 references to a 395-character entity in one attribute (340,434 bytes, billion-laughs factor 99.75): error 1 at the default limits (33,563,484 bytes tracked); accepted with factor 1,000 or a 128 MiB threshold. | Port: error 1 at the default limits (33,576,836 charged); equal traces in both relaxed runs. | none |
| Attribute expansion sizes (`attribute_expansion_sizes`) | 5 | 395 x 40,000 and 395 x 84,000 accepted; 395 x 85,500 and 300 x 120,000 refused (1); 2,000 x 20,000 stops with 43 first. | Same outcomes; equal traces where C accepts. | none |
| Content expansion control (`content_expansion_is_not_tracked`) | 1 | The same 33 MB expansion in content is streamed; C's total stays at 7,580 bytes; accepted. | Port accepts (20,932 charged); equal trace (text digest). | none |

All groups: 0 violations of the relation, 0 mismatches outside the
result line. Bounds: 512 KiB input, 60 s and 384 MiB heap per side for the
large groups (`LARGE` in `tracker_diff.rs`); measured heap peaks for the
tracker-only document 34.1 MB (C) / 52.3 MB (port), 0.16 s; whole file
3.3 s.

**Injected bugs** (2026-10-12, each reverted after the run, `tracker_diff`;
the exact mutations were re-run after a sandbox reset lost the first
session's working copy):

| # | Mutation | Detected by |
|---|---|---|
| K1 | `PoolShadow`: the current block grows by 1.5x instead of 2x (`tracker.rs`) | `tracker_only_attribute`, `attribute_expansion_sizes` |
| K2 | `AllocTracker::realloc` does not charge an increase (`tracker.rs`) | `tracker_only_attribute`, `attribute_expansion_sizes`, threshold sweep (12) |
| K3 | A new `TAG` structure not charged (`tracker.rs`) | threshold sweep, 34 violations (first: deep nesting) |
| K4 | `add_binding`: a new `BINDING` treated as reused, its structure not charged (`content.rs`) | threshold sweep, 6 violations (nested namespace scopes) |
| K5 | Factor setter accepts values >= 0.5 (`tracker.rs`) | `setters_match_c` |
| K6 | `charge_current` charges half of a string's growth (`mod.rs`) | `tracker_only_attribute`, `attribute_expansion_sizes` |
| K7 | `defineAttribute`: growth of the default-attribute array not charged (`dtd.rs`) | threshold sweep, 12 violations (distinct attribute names) |

K4 and K7 were first **not** detected: the seven original sweep documents
never needed enough bindings or default attributes for the undercharge
to exceed the creation margin. Two targeted documents were added (nested
namespace scopes: 600 nested prefixed elements, each declaring its own
prefix; distinct attribute names: 8 element types x 60 defaulted
attributes); both bugs are now caught, and the logs without the targeted
documents are kept with the injection logs.

Entity cases added in this PR: injected bug I2 of module 11b (the
proper-nesting check when an entity ends right after an inner reference)
had been caught only by the generated documents; three curated cases
(`async-open-then-empty-ref`, `async-open-then-text-ref`,
`async-inner-opens-at-end`, C error 13) now catch it in
`curated_entity_cases_match_c` (1,053 pairs, 0 mismatches).

**Fuzzing** (both targets: `parse_trace` now checks the tracker relation
with two extra tight runs per input; `buffer_diff` because the parser's
allocation paths changed). From the trimmed logs in
`docs/fuzz-logs/module-11c/` (commit 39a7005, strictly sequential slices):

| Target | Slices | Seconds | Executions | Findings |
|---|---|---|---|---|
| `parse_trace` | 7 | 672 | 1,221,587 | none (coverage 15,006 -> 15,042 edges) |
| `buffer_diff` | 7 | 617 | 4,269,530 | none (coverage 12,801 -> 12,855 edges) |

Merged corpora: `parse_trace` 12,638 files, `buffer_diff` 10,451 files
(docs/FUZZING.md), scanned for personal data (no hits). A first step-C run
on commit 7762e9d (parse_trace 665 s / 1,257,130, buffer_diff 660 s /
4,667,919 executions, no findings) is not counted: its logs were lost
with the sandbox before they were committed.

### Module 11d step D1 (external entity test handlers): handler calls versus C

Ported: the call sites of `XML_ExternalEntityRefHandler` and
`XML_NotStandaloneHandler` in content (`doContent` 3724-3743), at the end of
the doctype (external subset, 5702-5745), before the first element
(`XML_UseForeignDTD`, 5754-5797), at parameter entity references
(6382-6418) and in entity values (7186-7206), `getContext` (7685-7769) for
the context string, and `Parser::use_foreign_dtd`. The oracle gained test
handlers (`ParseOptions::ext_handler` None / Fail / Accept,
`not_standalone`, `use_foreign_dtd`, D-031). **The library never fetches
anything**: an external entity reaches only the caller's handler. Reading
an entity's bytes through an external entity parser is step D2.

Local run (2026-10-12, aarch64 Linux, commit 7d8cd76, `--release`):
`cargo test -p expat-difftest --test external_diff -- --nocapture`:

| Group (test) | Pairs | C behaviour | Security policy | Known divergences |
|---|---|---|---|---|
| Curated cases x matrix (`curated_external_cases_match_c`) | 1,863 (23 documents x 81 option sets: 3 handler modes x 3 not-standalone modes x 3 PE-parsing modes x {whole, 1-byte + namespaces, 7-byte buffer API + foreign DTD}) | Handler called 852 times, not-standalone handler 171 times; results 0 (1,170), 21 handler error (381), 22 not standalone (81), 16 external entity in attribute (75), 15/10 (156). External subset and external PEs are offered only when PE parsing is not NEVER *and* a handler is set; the not-standalone handler is called only for a non-standalone document, and after an external PE only if an external entity parser actually read it (`paramEntityRead`). | Same call sites, same arguments, same errors; nothing is read by the library. | none |
| Pinned behaviour (`pinned_external_behaviour`) | 7 | No handler: nothing read, success. Failing handler: 21 (`extref x ~ x.xml ~`). External subset: NULL context, name `#` never shown. Namespace context: `=default`, then bound prefixes in hash-slot order (incl. `xml`), form-feed separated. Open entities listed in slot order. NEVER + refusing not-standalone handler at an external subset: 22. | Same. | none |
| Generated documents (`generated_documents_with_handlers_match_c`) | 6,000 (random handler mode, not-standalone mode, PE parsing, foreign DTD, namespaces, chunking, buffer API) | 16 distinct results incl. 21 (178) and 22 (244); 360 handler calls. | Same. | none |

All groups: 0 mismatches. The context string's `m_tempPool` growth is
charged to the tracker (D-030).

**Injected bugs** (2026-10-12, each reverted after the run, logs in the
working copy `inject-logs/d1`):

| # | Mutation | Detected by |
|---|---|---|
| J1 | Content: the external entity reference handler is never called (`content.rs`) | all three tests |
| J2 | `getContext` separator `;` instead of form feed (`entity.rs`) | all three tests |
| J3 | The not-standalone check is skipped (`entity.rs`) | all three tests |
| J4 | The foreign DTD is not offered when there is no doctype (`prolog.rs`) | `curated_external_cases_match_c` only (generated documents always have a doctype) |
| J5 | The referenced entity is not marked open while the context is built (`entity.rs`) | all three tests |

### Module 11d step D2 (external entity parsers): child parsers versus C

Ported: `XML_ExternalEntityParserCreate` (`xmlparse.c` 1680-1863) as
`parse::ExternalEntities::create` / `ExternalEntityParser` (`parse/external.rs`,
D-032), `dtdCopy` for general entity parsers, `setContext` (7771-7840),
`externalEntityInitProcessor` 1-3 (text declaration, then content at tag
level 1), `externalParEntInitProcessor` / `externalParEntProcessor`,
`entityValueInitProcessor` / `entityValueProcessor` (a parameter entity
referenced in an entity value), `ignoreSectionProcessor` /
`doIgnoreSection` (conditional sections), `processXmlDecl` in its
text-declaration mode, and the sharing of the root parser's allocation
tracker and billion-laughs accounting (`getRootParserOf`) with every child.
`Stop::DtdNotPorted` is gone: every document now parses to C's result.
The oracle's test handler mode `ExtHandler::Read` (and the port's) looks
the system identifier up in an in-memory map, reads the bytes through an
external entity parser fed like the document and traces
`extend <ok> <code> <line> <column> <index>`; nesting is capped at 16
children (`extdepth`), a refused creation traces `extcreate`. **The
library never fetches anything.** All 540 upstream vectors are compared
in full under every option set (none stops early any more).

Local run (2026-10-12, aarch64 Linux, commit 01f20bf, `--release`):
`cargo test -p expat-difftest --test external_parser_diff -- --nocapture`:

| Group (test) | Pairs / runs | C behaviour | Security policy | Known divergences |
|---|---|---|---|---|
| Curated cases x matrix (`curated_external_parser_cases_match_c`) | 2,025 (75 documents x 27 option sets: 3 not-standalone modes x 3 PE-parsing modes x {whole, 1-byte + namespaces, 7-byte buffer API}) | General entities (text, elements, empty, BOM only, BOM + text declaration, text declarations with/without version, encoding, standalone, Latin-1, unknown encoding, late text declaration, unclosed/mismatched/parent-closing tags, invalid and partial characters, nesting, self/mutual recursion through the context string, namespaces inherited through the context, CDATA/PI/comment, unparsed and attribute references, UTF-16), external subsets and parameter entities (declarations, text declarations, BOM, INCLUDE/IGNORE incl. nested and PE keywords, unclosed sections, stray `]]>`, incomplete declarations, PE references inside declarations, nested and recursive PEs, PEs in entity values incl. character references, text declarations and tags, standalone yes/no, foreign DTD). Results 0 (1,020), 21 (660), 22 (219), 11 (72), 16 (27), 24 (27); 1,884 child parses, 18 distinct child results (e.g. 12 recursive reference 117, 13 async entity 54, 31 text declaration 72, 29 incomplete PE 36). | Same processors, same errors and positions in parent and child; a child shares the root's tracker and accounting; nothing is read by the library. | none |
| Pinned behaviour (`pinned_external_parser_behaviour`) | 9 | Entity content appears in place (`extend 1 0 1 22 22`); recursion through the context string: child error 12; an end tag of the parent inside the entity: 13; a text declaration without encoding: 31; subset declarations used by the document; IGNORE skipped with nested brackets; a PE in an entity value inside the subset; a character reference there is an invalid token (4) for C's prolog scan while the declaration handler still sees the value stored so far; NEVER reads nothing and skips the undeclared entity. | Same. | none |
| Nesting cap (`nesting_cap_matches_c`) | 6 (a chain of 20 external general entities x 3 PE modes x namespaces on/off) | The handler stops at 16 nested children (`extdepth` once, 16 `extend` lines); libexpat itself has no nesting limit for external entities, only the application's handler does. | Same; the cap is the harness's, as an application would set one. | none |
| Generated documents (`generated_external_documents_match_c`) | 6,000 (`docgen::external_case`: random DTDs with random external entity bodies, handler, not-standalone and PE modes, namespaces, chunking, buffer API) | Results 0 (2,200), 21 (3,049), 22 (606), 11 (145); 4,205 child parses. | Same. | none |
| Billion laughs across external entities (`billion_laughs_across_external_entities`) | 10 (5 shapes x {default limits, 4 KiB threshold + factor 100}) | Default limits accept all five: general 10^5 (11,111 child parsers, 87 ms), general 4^9 (87,381, 0.71 s), parameter 10^5 (11,111), 3^10 (29,524), 4^9 (87,381, 0.29 s), because every child's bytes are indirect but their total stays below the 8 MiB activation threshold. With a 4 KiB threshold the expansion stops with 43 inside a child after 120-692 child parsers and the parent ends with 21. C heap peak 38-94 KB. | Same results, positions and accounting counters; the port's tracker ends within 13,352 bytes above C's (asserted: never below C, at most one creation bound above) independent of the number of children. | none (see the fix below) |
| Tracker relation (`tracker_relation_across_external_parsers`) | 4,725 (75 curated + 1,500 generated documents, 3 runs each) | C refuses in 3,150 tight runs. | Port refuses in all of them; 0 port-only refusals; smallest margin 13,312 bytes. | DV-1 (margin only) |
| External subset growth (`tracker_keeps_external_subset_growth`) | 2 + relation (6 runs) | A subset declaring 3,000 entities: C's tracker ends at 429,020 bytes (the DTD growth stays charged to the root after the subset's parser is freed); C refuses in 4 tight runs. | Port 442,372; refuses in all 4; 0 port-only. | DV-1 (margin only) |

All groups: 0 mismatches, 0 relation violations; whole file 1.4 s. The
port's heap peak in the billion-laughs group (up to 16.9 MB) is the
in-process trace buffer (10.3 MB of trace for 87,381 children); the C
figure counts only the parser's heap.

**Found and fixed while documenting** (commit 01f20bf): freeing an
external parameter entity parser first released only its creation
charge, so the structures it owned (its `m_tempPool` blocks, open-entity
structures, ...) stayed charged to the root tracker. Measured, this was
not a constant margin but about 1 KB per child parser: 11,111 nested PE
parsers left 12.2 MB charged (C: 7.7 KB), and 87,381 left 67 MB, so the
port stopped with `XML_ERROR_NO_MEMORY` at the default limits where C
succeeds. The port now also releases what the child owns itself
(`Tracker::own_bytes`: temporary pools, tags, bindings, `m_nsAtts`,
`m_groupConnector`; plus open-entity structures and `m_atts` growth),
keeping only the shared DTD's growth charged, as C does. The tracker no
longer grows with the number of child parsers (87,381 PE parsers: C
8,028, port 21,380), and the billion-laughs test asserts it.

**Injected bugs** (2026-10-12, each reverted after the run, logs in the
working copy `inject-logs/d2`):

| # | Mutation | Detected by |
|---|---|---|
| E1 | A PE parser's DTD is not handed back to the parent | 5 of 7 tests |
| E2 | Child bytes counted as direct instead of indirect (`mod.rs`) | `billion_laughs_across_external_entities` |
| E3 | `setContext` does not mark the context's entities open | 6 of 7 tests |
| E4 | `dtdCopy` drops default attributes | curated, generated, tracker relation |
| E5 | The child's tag level does not start at 1 | 5 of 7 tests |
| E6 | `doIgnoreSection` does not wait for more input at a partial token | curated, generated |
| E7 | A text declaration parsed as an XML declaration | 4 of 7 tests |
| E8 | The child's tracker count is not returned to the root | first **not** detected; `tracker_keeps_external_subset_growth` was added and catches it |
| E9 | `paramEntityRead` not set after a PE was read | curated, generated, tracker relation |
| E10 | The entity-value prefix is not passed to the PE parser | 4 of 7 tests |
| E11 | A BOM at the end of the buffer does not wait for more input | generated |
| E12 | A PE parser's own structures are not released when it is freed (the bug fixed above) | `billion_laughs_across_external_entities` |
| E13 | Its open-entity structures are not released | `billion_laughs_across_external_entities` |
| E14 | Over-release: three times its own structures plus 16 KiB | tracker relation, subset growth, billion laughs |

Two further E11 variants (an undecided partial character or token
accepted at the end of an entity) were not detected; analysis showed that
`initScan` can never return those cases there, so the branches were
removed (`content_undecided` keeps only the cases `initScan` can return)
instead of being tested.

**Fuzzing** (`parse_trace`: inputs with `EXTERNAL_MARKER` carry entity
bodies read through external entity parsers, 2,300 such inputs in the
working corpus; `buffer_diff` because the parser's DTD paths changed; it
sets no external entity handler). From the trimmed logs in
`docs/fuzz-logs/module-11d2/` (strictly sequential slices):

| Target | Commit | Slices | Seconds | Executions | Findings |
|---|---|---|---|---|---|
| `parse_trace` | 65d49b8 | 7 | 681 | 986,184 | none (coverage 16,789 -> 17,053 edges) |
| `buffer_diff` | 65d49b8 | 7 | 702 | 4,596,334 | none (coverage 12,825 -> 12,856 edges) |
| `parse_trace` | 01f20bf (after the fix above) | 6 | 607 | 795,960 | none (coverage 17,071 -> 17,177 edges) |

`buffer_diff` was not re-run after 01f20bf: the fix changes only the
freeing of external parameter entity parsers, which that target never
creates. Merged corpora: `parse_trace` 13,869 files, `buffer_diff` 10,766
files (docs/FUZZING.md), scanned for personal data (no hits).

### Module 11 step E: coverage of the DTD code

Measured with `cargo-llvm-cov 0.9.1` (toolchain 1.99.0, aarch64 Linux,
`--release`), on the code of commit ccf66a1 (= main fdd4182 after PR #11):
`cargo llvm-cov --no-report --release` for the `expat` unit tests and for
every `expat-difftest` integration test target, then
`cargo llvm-cov report --release --ignore-filename-regex
'(tests/|crates/expat-oracle|benches/|fuzz/|vendor)'`. Not included:
doctests, and `tok_diff::upstream_vectors_every_offset` (it exceeds the
two-minute command limit of the measuring machine when instrumented; it
exercises the tokenizer, not the DTD code). The report warns that 65
functions have mismatched profile data (the same generic functions
instantiated in different test binaries); the numbers were identical with
and without the unit-test profile. Raw table (one run):
`docs/coverage/module-11-ccf66a1.txt`. Re-measured on Linux, macOS and
Windows in CI with the full suite, see "Coverage on every CI platform":
7 of the 388 uncovered lines (`tok::unknown::done`) were an llvm-cov
reporting artefact; corrected 94.95 % lines.

`crates/expat` overall:

| Metric | Covered | Total | % |
|---|---|---|---|
| Lines | 7,164 | 7,552 | 94.86 |
| Regions | 12,787 | 13,441 | 95.13 |
| Functions | 560 | 608 | 92.11 |

(Module 10: 92.59 % lines, 93.97 % regions, 92.29 % functions.)

The DTD modules:

| File | Lines | Regions | Functions |
|---|---|---|---|
| `parse/dtd.rs` | 268 / 269 (99.63 %) | 410 / 421 (97.39 %) | 22 / 22 (100 %) |
| `parse/decl.rs` | 704 / 744 (94.62 %) | 1,341 / 1,430 (93.78 %) | 21 / 25 (84.00 %) |
| `parse/model.rs` | 80 / 82 (97.56 %) | 108 / 111 (97.30 %) | 5 / 5 (100 %) |
| `parse/entity.rs` | 352 / 361 (97.51 %) | 543 / 567 (95.77 %) | 24 / 26 (92.31 %) |
| `parse/tracker.rs` | 473 / 500 (94.60 %) | 749 / 803 (93.28 %) | 52 / 58 (89.66 %) |
| `parse/accounting.rs` | 34 / 36 (94.44 %) | 37 / 39 (94.87 %) | 3 / 3 (100 %) |
| `parse/external.rs` | 565 / 614 (92.02 %) | 1,008 / 1,124 (89.68 %) | 25 / 26 (96.15 %) |

For reference: `parse/content.rs` 94.82 %, `parse/prolog.rs` 95.07 %,
`parse/mod.rs` 81.51 % lines.

Notable uncovered areas (from `--show-missing-lines`):

- **Suspend and abort** (`Parsing::Suspended` / `Finished`, `reenter`) in
  `content.rs`, `prolog.rs`, `external.rs`: reachable only through
  `XML_StopParser` / `XML_ResumeParser`, which are module 12.
- **Allocation refusals at single points**: the `NoMemory` returns of
  `dtdCopy` (attribute ids, element types, default attributes, entities),
  `setContext` (temporary pool, prefix lookup, bindings), the child
  creation, `storeEntityValue`, `processEntity`'s `OPEN_INTERNAL_ENTITY`,
  and the tracker's tag, binding, `m_nsAtts`, scaffold and group-connector
  refusals. The tracker relation and the threshold sweeps reach refusals
  in aggregate, but not each point.
- **`m_nsAtts` growth** (`Tracker::ns_atts` realloc beyond 8 slots, and
  the `m_nsAtts` term of `Tracker::own_bytes`, which an external parameter
  entity parser never has). Uncovered on ccf66a1. Since PR #18 (Part 2,
  "`m_nsAtts` growth", `ns_atts_diff.rs`) the growth is compared by trace
  (350 documents x 9 option sets = 3,150 comparisons) and its tracker
  charge one way by a threshold sweep (9 documents, both namespace
  separators: wherever C stops with `XML_ERROR_NO_MEMORY` the port stops
  too; the port may stop earlier). Still not compared: the table's size
  itself, an injected refusal at the table's own reallocation, and the
  `m_nsAtts` term of `Tracker::own_bytes` (`tracker.rs` 560, 0 hits in
  PR #18's CI coverage run 38299714072).
- **`XML_UnparsedEntityDeclHandler`** (`decl.rs` 487-493): on ccf66a1 the
  harness traced unparsed entities through `XML_EntityDeclHandler` only,
  so the port's call site of the unparsed-entity handler was not compared.
  Since PR #18 (Part 2, "`XML_UnparsedEntityDeclHandler`",
  `unparsed_diff.rs`) it is compared with `ParseOptions::unparsed_handler`
  on and off: 20 internal-subset documents x 7 option sets, and 4
  external-subset / external parameter entity documents x 7 option sets x
  3 parameter-entity modes. Only these tests set the handler; the
  generated tests and the fuzz targets leave it off.
- **Error branches of the external processors**: invalid, partial and
  partial-character tokens right after a text declaration in
  `entityValueInitProcessor` and `externalParEntInitProcessor`, an
  amplification breach inside an IGNORE section, the undecided-encoding
  paths of an entity whose encoding is detected late.
- **Defensive guards**: `XML_ERROR_UNEXPECTED_STATE` returns in `decl.rs`
  and `entity.rs` and `external.rs`, `None` branches of entity lookups,
  the input-length overflow guard of `XML_Parse`; the default no-op bodies
  of the `Handler` trait and getters such as `id_attribute_index` in
  `mod.rs` (the harness overrides every handler).

### Coverage on every CI platform (PR #13)

The CI job `coverage` (`.github/workflows/ci.yml`) measures `crates/expat`
on the three platforms of the `test` matrix with `scripts/coverage.sh`:
cargo-llvm-cov 0.9.1 (taiki-e/install-action), the pinned toolchain
1.99.0, an instrumented `--release` build, and the **full** test suite —
the two commands of the `test` job, `cargo test --workspace --all-features`
and `cargo test -p expat --no-default-features`, nothing skipped (this
includes `tok_diff::upstream_vectors_every_offset`, 108 s on the ubuntu
runner). The report filter is the one of module 11 step E (harness,
oracle, benches, fuzz, vendored C). Each platform uploads an artifact
`coverage-<OS>` with the summary table (txt and JSON), lcov, the uncovered
lines and llvm-cov's warnings, both raw and corrected (below). The
numbers here come from CI run 38220630479 (commit 2f6a37a; the library
code is that of main 81e02b3); raw tables: `docs/coverage/ci-2f6a37a-*.txt`.

**llvm-cov drops counts when it reports several test binaries together** (D-033).
A test binary that links a function without calling it carries a dummy
coverage record for it (function hash 0). llvm-cov looks each record up
by (name, hash); for the dummy, LLVM's `IndexedInstrProfReader` answers
`hash_mismatch` only if a profile record of that name has bit 60 of its
hash (the context-sensitive flag) equal to the dummy's, i.e. clear;
otherwise it answers `unknown_function`, llvm-cov gives the dummy zero
counts and registers it as the function's record, and the real record of
any binary loaded later is skipped as a duplicate (`RecordProvenance` in
`CoverageMapping.cpp`). About half of all hashes have bit 60 set, so the
result depends on which binary is loaded first. Reproduced locally:
`tok::unknown::done` has 4,439,354 executions in the profile and is
reported as covered with the `tok_diff` binary alone, and as never
executed once `core_diff` (which links it unused) is added in front.
`scripts/coverage_fix_profdata.py` adds, for every profile name whose
hashes all have bit 60 set, a record with hash 1 and one counter; no
mapping has hash 1, so its only effect is that the dummy lookups end in
`hash_mismatch` and the dummies are dropped like the others. On a local run of a
subset of the test binaries (aarch64 Linux), the corrected line set
equals the union of the reports of every binary taken alone (8,290
covered lines over all files, 0 differences; the raw report lost 16). The correction adds no test, removes no code from the report
and changes no count that llvm-cov did read; the raw report is kept next
to it.

The "N functions have mismatched data" warning (65 in module 11 step E,
117 / 107 / 105 raw on Linux / macOS / Windows, 216 on every platform
after the correction) is the harmless half of the same mechanism: every
listed record (`llvm-cov -dump`, artifact `*-mismatch.txt`) has hash 0,
i.e. it is a dummy of a function that another binary executed, and the
real record is used. No mismatch with a non-zero hash occurs.

`crates/expat`, full suite, corrected (raw in brackets where it differs):

| Platform | Lines | Regions | Functions |
|---|---|---|---|
| Linux (ubuntu-latest, x86_64) | 7,171 / 7,552 (94.95 %) [7,164, 94.86 %] | 12,791 / 13,441 (95.16 %) [12,787, 95.13 %] | 561 / 608 (92.27 %) [560, 92.11 %] |
| macOS (macos-latest, aarch64) | 7,171 / 7,552 (94.95 %) | 12,791 / 13,441 (95.16 %) | 561 / 608 (92.27 %) |
| Windows (windows-latest, x86_64 MSVC) | 7,169 / 7,552 (94.93 %) [7,162, 94.84 %] | 12,784 / 13,439 (95.13 %) [12,780, 95.10 %] | 561 / 608 (92.27 %) [560, 92.11 %] |

The DTD modules (identical on Linux and macOS; Windows differs only where
marked):

| File | Lines | Regions | Functions |
|---|---|---|---|
| `parse/dtd.rs` | 268 / 269 (99.63 %) | 410 / 421 (97.39 %); Windows 409 (97.15 %) | 22 / 22 (100 %) |
| `parse/decl.rs` | 704 / 744 (94.62 %) | 1,341 / 1,430 (93.78 %) | 21 / 25 (84.00 %) |
| `parse/model.rs` | 80 / 82 (97.56 %) | 108 / 111 (97.30 %) | 5 / 5 (100 %) |
| `parse/entity.rs` | 352 / 361 (97.51 %) | 543 / 567 (95.77 %) | 24 / 26 (92.31 %) |
| `parse/tracker.rs` | 473 / 500 (94.60 %); Windows 471 (94.20 %) | 749 / 803 (93.28 %); Windows 745 (92.78 %) | 52 / 58 (89.66 %) |
| `parse/accounting.rs` | 34 / 36 (94.44 %) | 37 / 39 (94.87 %) | 3 / 3 (100 %) |
| `parse/external.rs` | 565 / 614 (92.02 %) | 1,008 / 1,124 (89.68 %) | 25 / 26 (96.15 %) |

**Linux re-run with the full suite versus 94.86 %.** The raw Linux figures
equal module 11 step E to the line (7,164 / 7,552, 12,787 / 13,441,
560 / 608): `upstream_vectors_every_offset` and the `--no-default-features`
run add no covered line (the `all-features` and `full` reports are equal
on every platform). The 7 lines, 4 regions and 1 function between 94.86 %
and 94.95 % are `tok::unknown::done` (`tok/unknown.rs` 282-288), lost by
the llvm-cov behaviour above. The raw macOS report already keeps it
(another binary order or other hashes on that platform), which is why it
showed 94.95 % without the correction.

**Differences between platforms** (corrected reports; the lcov line sets
of Linux and macOS are identical):

- No platform-specific code in `crates/expat`: it has no `cfg(target_*)`
  and no `cfg(unix/windows)`; Linux x86_64 and macOS aarch64 instrument
  the same 13,441 regions and cover the same lines.
- Windows instruments 2 regions fewer (`hash/mod.rs` 310 instead of 311,
  `parse/content.rs` 1,739 instead of 1,740; the same 7,552 lines). These
  are the only two files that use `core::ffi::c_ulong` (libexpat's hash
  type `unsigned long`), which is 32 bits on Windows (LLP64) and 64 bits
  elsewhere; the exact region was not identified.
- Windows leaves `parse/tracker.rs` 576-577 uncovered (the refusal when a
  tag buffer grows to the raw-name length, 3 executions elsewhere), 2 more
  regions there and 1 region in `parse/dtd.rs`. Probable cause (inferred, not verified per
  test): the tracker tests take their thresholds from C's tracked byte
  counts measured on the running platform (threshold at C's final count,
  bisected boundaries), and `long`/`unsigned long` (`XML_Size`, hash
  values) is 32 bits on Windows, so C's counts, the probed thresholds and
  the port's refusal points can differ there.
- The harness itself has one Windows switch (`tests/src/tok.rs`, the
  position wrap-around test starts at column 3 because C's `XML_Size` is
  32 bits there, D-016); it does not change library coverage.

**Doctests.** `cargo llvm-cov --doctests` needs nightly. With the nightly
of the run (rustc 1.101.0-nightly 080cc2714 2026-10-11) the coverage data
is not usable: on the same test suite it reports 64.90 % regions against
91.26 % lines (stable: 95.16 % / 94.95 %), e.g. `role/mod.rs` 58 % regions
with 99 % lines, and its llvm-cov aborts in `-dump`
(`docs/coverage/ci-2f6a37a-nightly.txt`). Compared on that nightly, the
doctests add no covered line and no covered region of the library (Linux
and macOS: 663 uncovered lines and 4,747 uncovered regions with and
without them; the 58 extra instrumented lines lie in the doctest examples
themselves).
The CI job therefore measures without doctests; `scripts/coverage.sh DIR
nightly-doctests` (and `nightly` as the control) remain for a later
nightly.

**Below 95 % lines** (PR #13's tree, 7,552 lines; current figures are in
Part 2, "Coverage on this branch"). Corrected, `crates/expat` was at
94.95 % lines on Linux and macOS (4 lines short of 95 %) and 94.93 % on
Windows; regions
are above 95 % everywhere, functions at 92.27 %. Nothing was excluded or
skipped to get there. The uncovered lines (`docs/coverage/ci-2f6a37a-linux.txt`,
381 lines) by area:

- `parse/mod.rs` (98 lines, 19 functions): the default no-op bodies of the
  `Handler` trait (the harness overrides every handler) and getters and
  public-API paths (`XML_Parse` length guard, suspend/finish states) that
  module 12 will reach.
- Suspend / abort / reenter branches in `content.rs`, `prolog.rs`,
  `external.rs` (module 12, `XML_StopParser`/`XML_ResumeParser`).
- Allocation refusals at single points in `decl.rs`, `entity.rs`,
  `external.rs`, `tracker.rs`, `pool`, `hash` (`NoMemory` returns of
  `dtdCopy`, `setContext`, child creation, `storeEntityValue`, tag /
  binding / `m_nsAtts` / scaffold growth), compared only in aggregate.
- `XML_UnparsedEntityDeclHandler` call site (`decl.rs` 487-493), not
  compared by the harness on that tree; compared since PR #18 by
  `unparsed_diff.rs` only (handler off in the generated tests and fuzz
  targets). `m_nsAtts` growth: see module 11 step E above.
- Error branches of the external processors after a text declaration and
  late encoding detection (`external.rs`), defensive
  `XML_ERROR_UNEXPECTED_STATE` returns and `None` lookups.
- Code that never runs at run time: the `const fn` table builder
  `chartab::build` (evaluated by the compiler; the tables are checked
  against C), `Default`/`Display` impls, `EncodingIndex::to_c` and
  accessors in `tok/init.rs`, the closure `Converter` impl in
  `tok/unknown.rs`.

**Finding while measuring (not fixed here).** The proptest
`init_diff::xml_decl_in_every_builtin_encoding` uses random seeds and
failed twice in the coverage job on Windows (nightly runs of runs
38220053354 and 38220630479): for an unknown encoding made from seed
`0x52f43d23123766f6`, `XmlParseXmlDecl` on `<?xml é?>` (`é` as the
bytes C3 A9) returns `bad` 7 in the port and 8 in C; for seed
`0xfe8d9b29cbd0ff22` on `<?xml noé ?>` 9 against 11. The first case
reproduces on aarch64 Linux with the pinned toolchain, so it is a port
divergence in `toAscii` / pseudo-attribute parsing for unknown encodings,
not a Windows or nightly effect. It is recorded in `docs/STATE.md` as an
open finding; fixing it changes library behaviour and is left to a
separate change. **Fixed** by PR #15 (merged as main a990a8c, D-035, section "Q-5"
below); post-merge main CI 38240812899 green on every job, including
`init_diff` on all three platforms.

### Module 11 summary: all test groups

One row per test group of module 11 (details, counts and injected bugs in
the module sections above). "Same" in the policy column means the port's
behaviour and defaults are C's (D-026).

| Step | Group (test) | C behaviour | Security policy | Known divergences |
|---|---|---|---|---|
| 11a | Upstream vectors, generated DTDs, attribute definitions, deep models (`parse_diff`) | All `doProlog` declaration roles; error codes 2, 4, 8, 10, 11, 14-18, 27, 28, 30, 32 reached; content models nested to 20,000 groups. | Same; no early stop remains (11d). | none |
| 11a | Repeated attribute definitions (`repeated_attribute_definitions_match_c`) | First definition wins (`defaultAttForName`). | Same. | none |
| 11b | Content, attribute values, default values (`entity_diff` `content-*`, `attr-*`, `default-*`) | Replacement text tokenised in place; external entities in attributes 16, unparsed 15, undeclared 11. | Same; expansion bounded only by the amplification limit. | none |
| 11b | Parameter entities (`pe-*`) | Expanded only when PE parsing is not NEVER; PE in an internal-subset entity value 10; 29, 24. | Default NEVER. | none |
| 11b | Recursion, nesting, deep chains and cycles | 12 at the closing reference, 13 for broken nesting; iterative expansion, 10,000 open entities on the heap. | Same; bounded inputs. | none |
| 11b | Generated entity graphs (6,000) | Random declarations and references in every context. | Same. | none |
| 11b2 | Setters, default limits, custom limits, threshold and amplification sweeps, quadratic blow-up, breach positions (`amplification_diff`) | 100.0 / 8 MiB defaults; 43 at exactly C's token, position and counters. | Same defaults, same setter validation. | none |
| 11b2 | Q-4 (`q4_epilog_name_at_end_of_buffer`) | C adds `NULL - s` to the direct counter (undefined behaviour), then error 9. | Port counts 0 bytes. | Q-4 (counter only, masked) |
| 11c | Setters, creation charge, upstream vectors, generated documents, sweeps, tracker-only attribute, attribute sizes, content control (`tracker_diff`) | 100.0 / 64 MiB; 3,040 bytes after creation; error 1 when the tracked total breaches; the input buffer is not tracked. | Whenever C refuses, the port refuses; at the defaults the port never refused a document C accepts in any test. | DV-1: the port can refuse within a 11,764-13,352-byte margin earlier (only seen with tightened limits); **pending owner decision** |
| 11d D1 | Curated, pinned, generated (`external_diff`) | Handler called at C's sites with C's arguments; 21 / 22 errors; foreign DTD. | Library reads nothing. | none |
| 11d D2 | Curated (2,025), pinned, nesting cap, generated (6,000) (`external_parser_diff`) | Child parsers: text declarations, conditional sections, PEs in entity values, recursion through the context. | Same; nothing read by the library. | none |
| 11d D2 | Billion laughs across external entities | 87,381 nested external parsers accepted at the defaults; 43 inside a child with a 4 KiB threshold. | Same (security note in DTD-SECURITY.md: only the handler can cap external parsers). | none (tracker margin asserted) |
| 11d D2 | Tracker relation, subset growth | DTD growth from the subset stays charged to the root. | As 11c. | DV-1 margin only |
| all | Fuzzing (`parse_trace`, `buffer_diff`) | Port vs C by trace, incl. DTDs, limits, tracker relation, external entity bodies. | — | none found |

**Remaining gaps** (module 11):

1. DV-1: owner decision 2026-10-12 is exact parity in stages (D-034);
   phase 1 (sizes, R1 fix) below, W1/W2 remain until the later stages.
2. Suspend / resume, the default handler, the unknown-encoding handler and
   `XML_GetInputContext` are module 12; their branches in the DTD code are
   uncovered. External entity parsers exist inside `expat::parse` but have
   no public API yet.
3. The `encoding` argument of `XML_ExternalEntityParserCreate` was never
   exercised (both test handlers passed NULL); since `ext_encoding_diff`
   it is (docs/EXTERNAL-ENTITY-ENCODING.md, no difference). The
   unknown-encoding handler branch remains module 12.
4. `XML_UnparsedEntityDeclHandler` is compared on this branch (PR #18;
   Part 2, "`XML_UnparsedEntityDeclHandler`", `unparsed_diff.rs`): with
   `ParseOptions::unparsed_handler` the shim and the port both trace
   `unparsed <name> <base> <systemId> <publicId> <notation>`, compared by
   canonical trace with the handler on and off, for 20 hand-written
   internal-subset documents under the option matrix (7 sets) and 4
   documents with an external subset or external parameter entity read
   through the test handler (7 option sets x 3 parameter-entity modes).
   Limits: only these hand-written documents set the handler; the
   generated tests and the fuzz targets leave it unset, so there
   unparsed entities still reach the entity declaration handler.
5. Allocation refusals are compared in aggregate (relation, sweeps), not
   at each allocation point (per-point fault injection: Part 2, "Not done
   here"). `m_nsAtts` growth is compared on this branch (PR #18; Part 2,
   "`m_nsAtts` growth", `ns_atts_diff.rs`): 350 documents (1-1,100
   prefixed attributes around each power of two to 1,024, with and
   without a duplicate expanded name, a grow-and-shrink and a nested
   document) x 9 option sets = 3,150 trace comparisons; the tracker's
   charge for the table's growth is checked one way by a threshold sweep
   on 9 of these documents with both namespace separators (wherever C
   stops with `XML_ERROR_NO_MEMORY`, the port stops too). Limits: the
   table's size is not observed directly, only its effect on the trace
   and on tracker refusals; a refusal at the table's own reallocation is
   not injected.
6. libexpat does not cap the number or depth of external entity parsers;
   the port replicates that (an application must cap them in its handler).
7. Coverage (CI, full suite, corrected for an llvm-cov artefact, PR #13's
   tree of 7,552 lines): 94.95 % lines on Linux and macOS, 94.93 % on
   Windows, below the Phase 4 target of 95 % (for the current figures,
   94.86 % on main and 95.10 % / 95.07 % on PR #18, see Part 2,
   "Coverage on this branch"); regions 95.16 % / 95.13 %, functions 92.27 %. Doctests
   are not measured (nightly coverage unusable; on nightly they add no
   covered line).
8. Out of scope: `XML_UNICODE` builds (D-002).

### DV-1 parity, phase 1 (D-034): structure sizes and the reverse violation R1

What was wrong: `BINDING` (48 vs C's 56) and `NAME_AND_DEFAULT_ATTRIBUTE`
(16 vs 24) were charged below C's `sizeof`, so the port accepted
documents C refuses at the default limits (DIVERGENCES.md R1). DV-1 was
wider than documented: besides the creation bound (W1) the pool slack can
cost a whole pool block (W2).

| Group (test) | Inputs | C behaviour | Port behaviour | Divergence |
|---|---|---|---|---|
| Sizes (`tracker_sizes`) | 16 structures and quantities: TAG, BINDING, PREFIX, ENTITY, CONTENT_SCAFFOLD, offsetof(BLOCK, s), ATTRIBUTE_ID, DEFAULT_ATTRIBUTE, NAME_AND_DEFAULT_ATTRIBUTE, NS_ATT, ELEMENT_TYPE, OPEN_INTERNAL_ENTITY, ATTRIBUTE, NAMED *, malloc header, int | `sizeof`/`offsetof` from the vendored `xmlparse.c` built for the CI platform (oracle). 64-bit Linux: 96, 56, 16, 64, 32, 12, 24, 24, 24, 24, 88, 48, 32, 8, 8, 4. | Computed from C's field lists (`parse::c_layout`): equal for 15; ELEMENT_TYPE 96 (known over-count, step 6 of D-034). An under-count fails on every platform. | ELEMENT_TYPE +8 (and NS_ATT +8 on Windows), fail-closed |
| R1 (`dv1_windows::r1_attribute_defaults`) | W1 document with 10,000 ATTLIST defaults, pads 187,657, 188,000, 188,322, 188,323, 188,456, 188,457 | Refuses the first three (end 35,211,416), accepts from 188,323. | Refuses the first five (end 35,224,776), accepts from 188,457. Before the fix it accepted all six. | none in the C-refuses direction; DV-1 W1 shape for 188,323-188,456 |
| W1 (`dv1_windows::w1_creation_charge`) | pads 330,745, 330,746, 330,878, 330,879 | Refuses at 330,745, accepts from 330,746 (end 67,117,916). | Refuses up to 330,878 (end 33,576,836), accepts at 330,879. | DV-1 (W1), pinned until step 3 |
| W2 (`dv1_windows::w2_pool_slack`) | 84,947 references + tail 362, 363, 366, 367 | Accepts 362-366 (end 33,563,484), refuses 367. | Accepts 362, refuses 363-367. | DV-1 (W2), pinned until step 2 |

Per platform (CI run 38223508229 at 4e3c255, `tracker sizes and DV-1
windows` step; the test bisects both boundaries on each platform):

| Platform | Sizes | W1: C / port accept from pad (width; margin) | R1: C / port (width; margin) | W2: C / port refuse from tail (width) |
|---|---|---|---|---|
| ubuntu-latest x86_64 | 15 equal, ELEMENT_TYPE 96 vs 88 | 330,746 / 330,879 (133; 13,352) | 188,323 / 188,457 (134; 13,360) | 367 / 363 (4) |
| macos-latest aarch64 | as Linux | 330,746 / 330,879 (133; 13,352) | 188,323 / 188,457 (134; 13,360) | 367 / 363 (4) |
| windows-latest x86_64 | 14 equal, ELEMENT_TYPE 96 vs 88, NS_ATT 24 vs 16 | 330,745 / 330,879 (134; 13,384) | 188,323 / 188,457 (134; 13,392) | 367 / 363 (4) |

C's tracker is 32 bytes lower on Windows x64 at the same point (W1 end
67,117,884 vs 67,117,916; the creation allocations differ by platform,
measured per event in the next stage), so C's W1 boundary is one
document lower there. The first CI run (38222937126) failed on Windows
because the test then hard-coded Linux's pads; it now bisects per
platform.

Fuzzing at 4e3c255 (`fuzz-slice5.sh`, logs kept in the working notes):
`parse_trace` 7 slices, 707 s, 899,500 executions; `buffer_diff` 7
slices, 711 s, 4,582,373 executions; no findings. Differential suites at
6d1e645 (release, aarch64 Linux): tracker_diff 9/9, external_parser_diff
7/7, amplification_diff 8/8, entity_diff 4/4, parse_diff 9/9,
upstream_vectors 5/5, buffer_diff 3/3, workspace all passed.

Injected bugs (logs: `inject-logs/phase1` in the working notes, not in the
repository): P1 `BINDING_SIZE` back to 6 words: caught by `tracker_sizes`
(not by `dv1_windows`, whose documents declare no namespaces); P2
`NAME_AND_DEFAULT_ATTRIBUTE_SIZE` back to 2 words: caught by both
(`R1 pad 187657: the port accepts a document C refuses`); P3
`ELEMENT_TYPE_SIZE` 10 words, P4 `NS_ATT_SIZE` 16, P5 one pointer removed
from the `TAG` field list: caught by `tracker_sizes`.

### Q-5 (D-035): unknown-encoding `XmlParseXmlDecl` error position and C's over-read

Found by the random test `init_diff::xml_decl_in_every_builtin_encoding`
(intermittent red CI on the coverage job, PR #13 / main CI 38229597972).

**Reproduction (same input, port and C).** Tokenizer level (the port's
`Parser` has no unknown-encoding handler before module 12):
`docs/evidence/q5/repro_xmldecl_unknown.rs`, copied to `tests/examples/`
and run with `cargo run -q --release -p expat-difftest --example
repro_xmldecl_unknown` (aarch64 Linux). Before the fix (main `dc871d8`,
`repro-port-before.txt`):

```
seed 0x52f43d23123766f6 doc "<?xml é?><a/>" decl end 10      (C3 lead 2, A9 lead 4)
  ns=false gte=false: port ok=false bad=Some(7) | C ok=false bad=Some(8)   (same for all 4 ns/gte)
seed 0xfe8d9b29cbd0ff22 doc "<?xml noé ?><a/>" decl end 13   (C3 lead 3, A9 lead 4)
  ns=false gte=false: port ok=false bad=Some(9) | C ok=false bad=Some(11)
```

After the fix (`repro-port-after.txt`): `bad=Some(8) | C bad=Some(8)` and
`bad=Some(11) | C bad=Some(11)`. C through the public API
(`docs/evidence/q5/q5_overread.c`, `XML_ParserCreate("x-q5")` + an
unknown-encoding handler with the same lead bytes and a converter giving
`I` for `C3 A9`, `plain.txt`): `<?xml \xC3\xA9?><a/>` gives
`XML_ERROR_XML_DECL` (30), byte index 8, line 1, column 7.

**What 7/8 and 9/11 are.** `bad` is the `badPtr` argument, which
`processXmlDecl` passes as `&parser->m_eventPtr` (`xmlparse.c` 5143-5145);
`XML_GetCurrentByteIndex64` is `m_parseEndByteIndex - (m_parseEndPtr -
m_eventPtr)` (2714-2722), so for a document parsed from offset 0 they are
byte indexes. Columns are character counts (`XmlUpdatePosition`,
`XML_GetCurrentColumnNumber64` 2831-2840): byte 8 is column 7 and byte 11
column 10, because `C3 A9` is one character for the tokenizer.

**Root cause.** `doParseXmlDecl` sets `end` before `?>` (`xmltok.c`
1069-1070); `parsePseudoAttribute` steps one byte at a time and calls
`toAscii(enc, ptr, end)` (915-923) on the trailing byte `A9`, which the map
makes a lead byte of 4. `unknown_toUtf8` (1273-1301) checks only
`*fromP == fromLim` (1281), hands the converter `A9 ? > <` (1286), steps
past `fromLim` (1291-1292); the converter gives a negative value (no
output), so C continues with `a` (byte 11) and returns `a`: no error at
byte 7, the next one at byte 8. The port stopped at `end` and returned
-1 at byte 7. Fix `fb35448e`: `Encoding::to_utf8_until` (the C loop on the
whole buffer, `fromLim` at `lim`), used by `to_ascii`.

**Memory safety (upstream finding Q-5).** When the declaration ends the
data, C reads past it: ASan (`asan.txt`, clang, the vendored 2.9.0, both
`XML_Parse` and `XML_GetBuffer` + `XML_ParseBuffer`) reports
`heap-buffer-overflow` READ in `unknown_toUtf8` `xmltok.c:1283` past the
buffer `XML_GetBuffer` allocated (`xmlparse.c:2603`), for both seed
documents ending at `?>` (the map makes ASan's fill byte `0xBE` a lead byte
converting to nothing, so the walk runs off the allocation: 2,048 bytes
for `XML_Parse`, 4,096 for `XML_ParseBuffer`). With only the two seed
lead bytes in the map, MSan (`msan.txt`) reports use of uninitialised
heap at `xmltok.c:1284`, and the converter is called on bytes after the
document (`plain.txt`, `<-- past the document`). The port: `#![forbid(unsafe_code)]`,
checked slice access only, stops with `InputIncomplete` where C leaves
the data; the parser hands processors `buffer[..buffer_end]`. Miri on the
port-only tests `tok::tests::q5` (`miri.txt`): 2 passed, no UB.
Divergence policy: DIVERGENCES.md Q-5 / D-035 (replicate inside the data;
past it the port reports the error at the trailing byte instead of
modelling indeterminate memory).

**Tests.** `tests/tests/xmldecl_unknown_diff.rs` (C-differential, 5 tests):
both seeds verbatim (4 ns/text-declaration variants each, scanners and
token walk); 34,944 declarations = 64 map pairs (lead of 2/3/4, malformed,
non-XML, 2- and 3-byte BMP, ASCII; 36 pairs C rejects, checked as rejected
on both sides) x 13 converter results (ASCII, 2/3/4-byte UTF-8, surrogate,
past Unicode, negative, 0) x 14 pseudo-attribute positions x 3 tails, each
with both NS modes and declaration kinds (`ok`, `bad` and its line/column,
version, encoding name, standalone); the same 14 declarations in all 12
built-in encodings; whole parses of them under the option matrix (error
code, line, column, byte index). `crates/expat/src/tok/tests.rs` `q5`:
port bounds tests (Miri). Mutation check (`mutation-check.txt`): with the
fix reverted, 3 of the 5 differential tests fail (the two
built-in-encoding tests are unaffected, as expected).

**Random test.** `PROPTEST_CASES=100000 PROPTEST_RNG_SEED=11` and
`95000` / seed 12 on the release build: ok, no mismatch
(`random-scan.txt`); earlier runs of the same test on the fixed library
(260,000 cases with seeds 1-4) also passed.

**Fuzzing** (aarch64 Linux, library `fb35448e`,
`docs/fuzz-logs/q5-unknown-xmldecl/SUMMARY.md`): `tok_diff` (seeded unknown
encodings, `XmlParseXmlDecl` on every declaration token) 7 slices, 707 s,
278,025 executions; `parse_trace` 6 slices, 616 s, 668,139 executions
(built-in encodings only, where `to_utf8_until` is the old conversion). No
crash, no mismatch.

### DV-1 parity, stages 2-8 (D-034): exact tracker parity

The port's tracker now replays C's allocations event by event (stages
2-7: pool model, creation, untracked C allocations, raw tag names,
`ELEMENT_TYPE`/`NS_ATT`, `XML_ParserFree`). Stage 8 came from making the
relation exact: with "port refuses, C accepts" counted as a failure and
the tight runs compared event by event, the committed `parse_trace`
corpus failed (408 inputs at first). Root causes, each fixed at C's
state without fake charges or looser limits:

| Class (first difference) | Example | C (`xmlparse.c`) | Port before | Fix (b699b1a) |
|---|---|---|---|---|
| Port refuses, C accepts, all with namespaces (192 inputs) | seed `0108d5df…`, UTF-16LE, threshold 7,070: events equal through `+24 refused` at 8331, then C reports `notation n2`, `result error 4`, the port `result error 1` | `getAttributeId` does not check the `lookup` of an `xmlns:` prefix (7650-7652): the id keeps a NULL prefix with `xmlns` set and the parse goes on; `storeAtts` treats the attribute as unprefixed (4262, 4295), `dtdCopy` copies `xmlns` only with a prefix (8011) | returned `XML_ERROR_NO_MEMORY` at the refusal | same state: `xmlns` set, no prefix, parse goes on |
| Entity value after a refused allocation (210 inputs) | `<!ENTITY foo '…'>`, first `entityValuePool` block refused: C `entity foo 0 ~`, port `entity foo 0 ` (empty); refused growth inside a value: C's value ends at the last whole character written | `XML_ROLE_ENTITY_VALUE` calls the handler with `poolStart`/`poolLength` before returning the error (5935-5958): NULL without a first block (7126-7129), otherwise what `poolAppend` wrote (whole characters, `utf8_toUtf8`'s trim); `poolAppendChars` (character references) writes nothing on failure | empty value; the whole last token | value NULL without a block; `PoolShadow::append_text` advances over the whole characters written before each growth and the value is cut to the pool's length |
| Release order after a refused tag buffer growth (6 inputs) | `<a>` then a long start tag, `+32 refused` at 3790: C frees the new tag first (`-40`, `-104` at 1885/1887), the port its parent | the tag is on `m_tagStack` (3764-3766) before its buffer grows (3790-3792), so `XML_ParserFree` (1874-1887) releases it first | put it on the free list | the tag stays the innermost open tag |

Named regressions (`tracker_stages.rs`):
`refusal_of_an_xmlns_prefix_is_tolerated` (threshold sweeps one byte
apart; C reaches the content after the refusal and reports the unbound
prefix), `refused_entity_value_reports_what_c_stored` (1- to 4-byte
characters and character references, UTF-8 and UTF-16LE; NULL and partial
values both occur), `refused_tag_buffer_growth_released_first`.
`assert_equal_streams` and `tracker_events` now compare whole traces, not
only the result lines (the entity values were invisible to them).

Injected bugs (mutations and logs in the working notes,
`inject-logs/blocker`): B1 a refused `xmlns:` prefix fatal again: caught
by `refusal_of_an_xmlns_prefix_is_tolerated` and `fuzz_corpus` (corpus
and seeds); B2 NULL entity value reported as empty: caught by
`tracker_stages` (2 tests), `fuzz_corpus` and `tracker_events`; B3 value
not cut to the pool: `refused_entity_value_reports_what_c_stored`,
`fuzz_corpus`; B4 `append_text` without the partial fill: the same two;
B5 refused tag back on the free list:
`refused_tag_buffer_growth_released_first`, `fuzz_corpus`.

Results on aarch64 Linux (local, release, at the merge of `main`):

| Check | Runs | Result |
|---|---|---|
| `tracker_events` (creation through `XML_ParserFree`, whole traces) | 15,966 runs, 5,160 with a refusal in C | 15,966 equal streams, equal peaks and traces |
| `tracker_stages` | 12 named tests | pass |
| `fuzz_corpus` under the exact relation | `parse_trace` 13,869 inputs (3 runs each: caller's options, threshold at C's end and half, events compared), `buffer_diff` 10,766, seeds | pass |
| `tracker_diff` | generated 4,500 runs (C refused 2,818), upstream 6,480 (3,680), boundary sweep 108 | port-only refusals 0, end margin 0 |
| `external_parser_diff` tracker relation | 4,725 runs (C refused 3,150) | port-only refusals 0, margin 0 |
| `dv1_windows` | W1, W2, R1 bisected | same boundary for C and port (width 0) |
| `tracker_sizes` | 22 sizes and offsets | all equal to C's |

Fuzzing at e1a4e6e (`fuzz-slice7.sh`, logs `docs/fuzz-logs/dv1-parity`):
`parse_trace` 7 slices, 658 s, 543,076 executions; `buffer_diff` 7
slices, 637 s, 3,531,508 executions; no findings. (The working corpora
were lost in a sandbox reset before they could be merged; the committed
corpora are unchanged.)

Fuzzing again on the merge with `main` (`a62e8b0`, which brought PR #15's
tokenizer change; `fuzz-slice7.sh`, logs `docs/fuzz-logs/dv1-merged`):
`parse_trace` 6 slices, 617 s, 609,475 executions; `buffer_diff` 6
slices, 606 s, 3,891,592 executions; no findings (every slice exit
status 0, no artifact). One further `buffer_diff` slice was cut short when
the local disk filled up and is not counted.

Per platform, from the pull request's CI run 38245716778 (head
`a62e8b0`; step "tracker sizes, events and DV-1 windows",
`cargo test --release ... -- --nocapture`). Test, coverage, lint and fuzz
smoke jobs passed on all three platforms; the Miri job did not finish
(still running after 80 minutes, against 1-3 minutes on `main`), see
"Miri and the pool shadow test" below.

| Check | x86_64 Linux | aarch64 macOS | x86_64 Windows (MSVC target) |
|---|---|---|---|
| `tracker_events`: equal streams creation → end of parse / → `XML_ParserFree` | 15,966 / 15,966 of 15,966 | 15,966 / 15,966 of 15,966 | 15,966 / 15,966 of 15,966 |
| equal peaks / equal results | 15,966 / 15,966 | 15,966 / 15,966 | 15,966 / 15,966 |
| runs with a refusal in C | 5,160 | 5,160 | 5,141 |
| `tracker_stages` | 12 / 12 pass | 12 / 12 pass | 12 / 12 pass |
| W1 boundary (C, port), width | 330,746, 330,746, 0 | 330,746, 330,746, 0 | 330,745, 330,745, 0 |
| W1 end count (C = port) | 67,117,916 | 67,117,916 | 67,117,884 |
| W2 boundary (tail, C, port), width | 367, 367, 0 | 367, 367, 0 | 367, 367, 0 |
| W2 end count at tail 366 (C = port) | 33,563,484 | 33,563,484 | 33,563,452 |
| R1 boundary (C, port), width | 188,323, 188,323, 0 | 188,323, 188,323, 0 | 188,323, 188,323, 0 |
| R1 end count (C = port) | 68,765,848 | 68,765,848 | 68,765,816 |
| `tracker_sizes` | 22 / 22 equal | 22 / 22 equal | 22 / 22 equal |

`tracker_sizes` (C's `sizeof`/`offsetof` from the vendored `xmlparse.c`
built by the oracle for the platform; port from `parse::c_layout`):

| Structure | x86_64 Linux | aarch64 macOS | x86_64 Windows |
|---|---|---|---|
| TAG, BINDING, PREFIX, ENTITY, CONTENT_SCAFFOLD | 96, 56, 16, 64, 32 | same | same |
| offsetof(BLOCK, s), ATTRIBUTE_ID, DEFAULT_ATTRIBUTE, NAME_AND_DEFAULT_ATTRIBUTE | 12, 24, 24, 24 | same | same |
| NS_ATT | 24 | 24 | **16** |
| ELEMENT_TYPE, OPEN_INTERNAL_ENTITY, ATTRIBUTE, NAMED *, malloc header, int | 88, 48, 32, 8, 8, 4 | same | same |
| `struct XML_ParserStruct` | 1,104 | 1,104 | **1,072** |
| DTD | 368 | 368 | 368 |
| offsetof(XML_ParserStruct, m_position) | 808 | 808 | **800** |
| ACCOUNTING, MALLOC_TRACKER, ENTITY_STATS | 40, 40, 24 | same | **32, 32, 16** |

Every value is equal between C and the port on each platform. The
Windows differences are C's own: the 32 bytes of `XML_ParserStruct` are
the four `unsigned long` members (`m_nsAttsVersion`, visible in the
`m_position` offset, and the `debugLevel` of `ACCOUNTING`,
`MALLOC_TRACKER`, `ENTITY_STATS`), 4 bytes instead of 8 plus alignment,
8 bytes each; `NS_ATT` holds two `unsigned long`s. The Windows test job
builds for `x86_64-pc-windows-msvc`, where the `cc` crate compiles the
oracle with MSVC; run 38245716778 did not print the compiler, so
`tracker_sizes` now prints the C compiler's identity from its predefined
macros and fails on an MSVC target unless the compiler is MSVC-ABI
(`expat_oracle::c_compiler`; aarch64 Linux locally: `gcc 14.2.0`). The
lower Windows end counts in `dv1_windows` are exactly those 32 bytes of
the parser structure (W1's boundary moves by one unit for the same
reason), and `tracker_events` runs fixed thresholds, so C itself refuses
in 19 fewer runs on Windows; C and the port agree run by run on every
platform. (That the W1 unit shift and the 19 runs come from the smaller C
structures is inferred from the 32-byte end counts and the fixed
thresholds; no other difference was measured.)

Miri and the pool shadow test. The Miri job of run 38245716778 sat in
`cargo +nightly miri test -p expat --all-features` for over 80 minutes
(`main`: 1-3 minutes). Cause: `parse::tracker::tests::
shadow_allocates_exactly_like_the_string_pool` (stage 2) runs 6,000 steps
on random texts of up to 12,000 characters, and Miri interprets every
character. Locally (aarch64, Miri for the host and for
`x86_64-unknown-linux-gnu`) it was still running after 2.5 and 3.5
minutes; the first 64 steps took 212 s and the first 33 steps 112 s
(about 3.4 s a step, so hours for 6,000). Natively the
test keeps its 6,000 steps; under Miri it runs the first 33 steps of the
same sequence, which already make the first block, its doublings from
1,036 to 16,396 bytes and a second first block that grows, and it asserts
at least five block growths (natively the full run adds seven more
requests, at steps 38 to 3,003). With that, the CI command passes
locally (aarch64): 49 unit tests and 16 doctests, 252 s wall time.

### External entity encoding (`ext_encoding_diff`, docs/EXTERNAL-ENTITY-ENCODING.md)

The harness option `ParseOptions::ext_encoding` passes an `encodingName`
to every `XML_ExternalEntityParserCreate` of the `Read` test handler, in
the C shim and in the port adapter alike (`None` = NULL, the previous
behaviour). `tests/tests/ext_encoding_diff.rs` records C and the port
separately per case and compares them: 4 entity contexts (general entity,
external subset, parameter entity, parameter entity inside an entity
value) x 42 bodies (UTF-8 / UTF-16LE / UTF-16BE with and without BOM,
ISO-8859-1, consistent / inconsistent / version-only / standalone / empty /
unknown text declarations, BOM vs declaration conflicts, child parse
errors) x 10 names x 4 parent encodings x 3 feedings (20,160 pairs), a
configuration matrix (parameter entity parsing, namespaces, triplets,
foreign DTD, standalone, not-standalone handler, handler modes;
138,252 pairs), nested children (120) and 24 pins of C's behaviour.
**No difference.** Six library mutations (BOM rules, declaration ignored
when a name is given, name ignored, entity-value text declaration) and a
harness mutation were each caught. C behaviour worth knowing: the parent's
encoding is never inherited; a UTF-16 BOM wins over an explicit UTF-8 /
US-ASCII / other-byte-order name (except ISO-8859-1 in content); a given
name makes the declared encoding irrelevant; a parameter entity inside an
entity value takes XML-declaration rules for its text declaration
(`processXmlDecl(parser, 0, ...)`). Not covered: the unknown-encoding
handler (module 12).

### Part 2: remaining C behaviour (branch `part2-finish`, PR #18)

Branch `part2-finish` (on top of `part2-unparsed`), PR #18, integrated
after PR #16 (merged as main `4a6cd19f`, CI run 38402332452) and PR #19
(merged as main `ecdec368`, CI run 38412206267).
Measured against the vendored libexpat 2.9.0 by canonical trace (events,
error code, line, column, byte index), under `diff::option_matrix` unless
stated. No library code was changed except the duplicate check below.

**`XML_UnparsedEntityDeclHandler`** (`tests/tests/unparsed_diff.rs`, 2
tests). C behaviour: with the handler set, an `NDATA` entity is reported
as `unparsed <name> <base> <systemId> <publicId> <notation>` and not to
the entity declaration handler; without it, the entity declaration handler
gets the notation; a duplicate declaration is not reported; the notation
is not checked (non-validating); declarations after an unread parameter
entity are skipped. 20 internal-subset documents (PUBLIC/SYSTEM, empty
system id, undeclared notation, duplicates in both orders, references in
content and attribute values, `NDATA` on a parameter entity, missing or
extra names, missing space, prefixed names, non-ASCII) with and without
the handler; an external subset and an external parameter entity read
through the test handler, 4 documents x 7 option sets x 3
parameter-entity modes x handler on/off. Shim: `ParseOptions::unparsed_handler`.
Reaches `decl.rs` 487-493 (the port's call site was never compared).
Mutation check: reporting entities named `u` to the entity handler
instead fails the test. Policy: replicate. No divergence.

**External entity bodies** (`tests/tests/external_body_diff.rs`). 76
bodies (empty, BOM only, text declaration complete or cut, unclosed
comment / tag / PI, partial and invalid characters, UTF-16 with and
without BOM, a start tag where only declarations are allowed, literals,
and 23 bodies that start with an XML declaration) read through the test
handler in 6 documents: referenced inside an entity value literal of the
external subset (`s.dtd`: the child starts in `entityValueInitProcessor`
because `storeEntityValue` sets `inEntityValue`), in place of an entity
value (`v.dtd`: an inner parameter-entity reference, so
`externalParEntInitProcessor`, not the entity-value processor), the
external subset itself, an external parameter entity in the internal
subset, and an external general entity in content (once and twice); 7
option sets (the child is fed in the same chunks) x 3 parameter-entity
modes = 9,576 comparisons, every one equal; more than a quarter end in a
child error by construction (asserted). C behaviour worth noting: inside
an entity value C calls `processXmlDecl(parser, 0, ..)`, so the
declaration is an **XML** declaration, not a text declaration: `version`
is required (`<?xml encoding='UTF-8'?>` fails with
`XML_ERROR_XML_DECL`, error 30, column 6 of the child) and `standalone`
is accepted. After it, `entityValueProcessor` (`external.rs` 750-778)
handles complete literals, an invalid byte (`XML_ERROR_INVALID_TOKEN`),
an unclosed comment or literal (`UNCLOSED_TOKEN`), a partial character
(`PARTIAL_CHAR`), the start of an element (`XML_ERROR_SYNTAX`) and, in
chunked parses, a token cut by the end of a chunk (it returns for more
input); every one of these branches was checked to run (temporary probes,
removed) and matches C. Also reaches `external.rs` 705-706, 792-793
(nothing scanned in a known encoding: empty value) and 819. Policy:
replicate. No divergence.

**`m_nsAtts` growth** (`tests/tests/ns_atts_diff.rs`, 3 tests). C
behaviour: the namespaced-attribute table doubles while `(nPrefixes << 1)
>> m_nsAttsPower` is non-zero and is reused across start tags through a
version counter; a duplicate expanded name (two prefixes bound to one URI)
is found only through it and reported as `XML_ERROR_DUPLICATE_ATTRIBUTE`
at the start tag. Start tags with 1-1,100 prefixed attributes, at, below
and above every power of two to 1,024, over 1, 3 and 17 prefixes, with no
duplicate or a duplicate first, in the middle or last; a document whose
start tags grow and shrink (stale entries of a reused table must not
count) and nested tags with the same names; 9 option sets (the option
matrix plus triplets with 64-byte chunks and the buffer API with 7-byte
chunks): 350 documents, 3,150 comparisons, every one equal, and every duplicate document
fails with error 8 under each namespace option set (1,036 such runs,
asserted). The
tracker charges the table's growth (`Tracker::ns_atts`): a threshold sweep
(amplification 1.0, geometric grid 1 KiB-4 MiB x 9/8 plus a binary search
for C's boundary, both namespace separators) on the 1,025- and
513-attribute tags and the grow-and-shrink document: wherever C stops with
`XML_ERROR_NO_MEMORY` the port stops too (module 11c relation; C refused
in 256 of the sweep runs, the port in each). Injected
bugs: comparing names with `==` instead of C's prefix test (triplets) and
growing the table one step late in the tracker are both caught.

**Finding (fixed here): quadratic duplicate check.** The port checked
each prefixed attribute's expanded name against a list of all earlier
ones (`content.rs`, storeAtts), where C uses a hash table: one start tag
with 80,000 prefixed attributes (1 MB) took 1.48 s in the port against
47 ms in C, 20 s at 4 MB, so input size bought quadratic CPU time (a
denial-of-service risk C does not have). The names are now grouped by
their SipHash in a `BTreeMap` (same duplicate rule: equal hash and the
stored name starts with the new one): 84 ms for the same tag.
Unprefixed attributes were not affected (they use attribute-id flags, as
in C). Regression test `ns_duplicate_check_scales_like_c`, measured against C
on the same machine: from 10,000 to 160,000 prefixed attributes (16
times) the port's time may grow at most three times as much as C's (best
of three runs each). aarch64 Linux: C x24.3-25.4, the fixed port
0.98-1.03 of C's factor in debug and release builds; the old list x214.9
(8.75 of C's factor), which fails. (A first, self-relative version of the
test -- 4x the attributes in under 9x the time -- measured x4.8-5.2 after
the fix and x15.6 before it: too close to its bound to be stable on loaded
CI runners, so it was replaced.) Behaviour is unchanged:
the full suite and the new C-differential tests pass. Injected bugs in
the new check (`ns_atts_diff`, `parse_diff`; logs kept outside the repo):
N1 exact equality instead of C's prefix test -- caught by
`ns_atts_growth_matches_c` and `parse_diff::namespace_triplets_match_c`;
N2 duplicate check removed -- caught by both of those and
`ns_atts_tracker_relation`; N3 names filed under a wrong hash key --
caught by the same three; N4 the old quadratic list -- caught by
`ns_duplicate_check_scales_like_c` only (no output changes); the scaling
test on the CI runners is under "Scaling report" below. The
committed fuzz corpora (`fuzz_corpus` replay) caught none of N1-N3: they
held no start tag with two prefixes bound to one URI. 40 curated
`parse_trace` inputs for this were added (`docs/FUZZING.md`, Part 2);
with them the replay test `parse_trace_corpus_matches_c` catches N1, N2
and N3 as well. No divergence
number is needed (no observable output changed); listed here as a
finding.

**CI note.** In PR #15's run 38237867506 (attempt 1) the ubuntu coverage
job aborted inside the `llvm-cov report -dump` diagnostic of
`scripts/coverage.sh` (glibc `corrupted size vs. prev_size`, SIGABRT)
after the reports were written; the re-run of the same commit (job
114777514198) and post-merge main 38240812899 were green. The root cause was
found later on branch `q5-verification` (PR #17, open, not merged): a
data race in `llvm-cov report -dump`. `scripts/coverage.sh` on this branch
(PR #18) now runs that diagnostic with `-dump -num-threads=1` (commit
1445f800, cherry-picked from PR #17's ec9cf645; only the evidence comment
differs, pointing to PR #17, branch `q5-verification`,
`docs/evidence/llvm-cov-dump-race/`). PR #17 still holds the original
commit and the evidence. The summary, missing-lines, JSON and lcov calls
are unchanged. PR #18's CI run 38299714072 on that commit: all 9 jobs
green; the Ubuntu coverage `-dump` step completed without SIGABRT; 95.10 %
lines. A single successful run does not prove that the race is gone, since
the earlier failure was intermittent.

**Scaling report** (`ns_duplicate_check_scales_like_c`). The test prints
one `ns-scaling:` line on stderr on every run (input sizes, best-of-3 C
and port times, growth factors, port/C ratio, limit, runner; visible on a
passing run only with `--nocapture`). When the environment variable
`NS_SCALING_REPORT` names a file, it writes the same values as JSON:
`test`, `attributes` and `input_bytes` (`small`, `large`), `c_seconds` and
`port_seconds` (`small`, `large`), `growth` (`port`, `c`, `port_over_c`,
`limit_port_over_c`), `passed`, `runs_per_measurement`, `statistic`
(minimum wall-clock seconds of the runs) and `runner` (`os`, `arch`,
`family`, `cpu`, `available_parallelism`, `debug_assertions`). The limit
is one constant (`LIMIT_FACTOR` = 3.0) used for the check and the report.
CI: the `test` job (all three OSes) has a step that runs this test alone
with `--nocapture` and `NS_SCALING_REPORT`, followed by
`actions/upload-artifact@v4` with artifact `ns-scaling-<os>`,
`if: always()` and `if-no-files-found: error`. Failure modes, verified
locally (aarch64 Linux, scratch changes reverted): if the JSON cannot be
written the test fails; if both the check and the write fail, the
original assertion message is kept with the write error appended as
"(also: ...)"; a failed check still writes `passed: false`.

CI run 38289234285 (PR #18, debug build, best of 3, 10,000 -> 160,000
prefixed attributes, 118,910 -> 2,128,910 bytes), from the JSON
artifacts:

| Runner | C small / large | Port small / large | C growth | Port growth | Port / C (limit 3.0) |
|---|---|---|---|---|---|
| ubuntu-latest (x86_64, AMD EPYC 7763, 4 threads) | 6.105 / 141.586 ms | 11.606 / 227.421 ms | x23.2 | x19.6 | 0.84 |
| macos-latest (aarch64, Apple M2 Pro (Virtual), 5 threads) | 3.690 / 83.492 ms | 6.070 / 134.180 ms | x22.6 | x22.1 | 0.98 |
| windows-latest (x86_64, AMD Family 25, 4 threads) | 5.182 / 124.887 ms | 11.702 / 289.651 ms | x24.1 | x24.8 | 1.03 |

The test passed on all three. Not tested: the missing-report path of
`if-no-files-found: error` was never executed (checked only against the
`action.yml` documentation of upload-artifact v4); whether the artifact
name collides when a job is re-run; the measurement order changed with
the report (small input first, then large; before, large first) and its
effect was not measured. The timing test keeps a low but non-zero risk
of flaking: in 20 local runs (aarch64 Linux) the ratio ranged 0.54-1.42
against the limit of 3.0 (the old quadratic check measured 8.72 locally).

**Coverage on this branch** (CI run 38289234285, `crates/expat`, full
suite, same pipeline as main): lines 95.10 % on Linux and macOS
(7,238 / 7,611) and 95.07 % on Windows (7,233 / 7,608); main CI run
38240812899 on a990a8ca measured 94.86 % (7,218 / 7,609, Linux). The
94.95 % of "Coverage on every CI platform" was measured on an older tree
(7,552 lines) and is not comparable with these figures.

**Fuzzing** (with the duplicate-check fix, `docs/fuzz-logs/part2`):
`parse_trace` 7 slices, 677 s, 783,495 executions (committed corpus plus
40 working-corpus seeds of start tags with two prefixes bound to one
URI, with and without triplets); `buffer_diff` 7 slices, 672 s, 4,296,352
executions. No crash, no mismatch. aarch64 Linux only.

**Not done here.** OOM fault injection per allocation point
(PR #16, DV-1 parity phase 2, has merged, so the port's allocation
points are final; not started). A libFuzzer merge of the Part 2
runs' working corpora into `fuzz/corpus` (only the 40 curated inputs
were added). Suspend/resume: see "Remaining uncovered
lines".

## Divergences

`docs/DIVERGENCES.md`. DV-1 (module 11c, D-030: the allocation tracker
could refuse slightly earlier than C) is removed by exact parity (D-034,
section above; per-platform status there); R1 (the reverse direction)
was a defect and is fixed. No divergence in parse results. Five C quirks found
by the harness: Q-3 resolved as "replicate" (module 7; the input violates
XML 1.0 §4.3.3), Q-1 and Q-2 resolved as "replicate" (module 10, D-024),
Q-4 (module 11b2, D-029: undefined behaviour in C's epilog accounting that
corrupts only an internal counter) not replicated, masked in the harness;
Q-5 (D-035: C's unknown-encoding `toAscii` reads past the data, an
out-of-bounds read shown with ASan/MSan) replicated inside the data, not
outside it.
Platform-dependent C behaviour fixed to one choice: D-016 (counter width),
D-017 (sign of stray bytes in `utf8_toUtf16`).

## Benchmarks

Only an oracle baseline exists (`cargo bench -p expat-bench`); port-vs-C
comparisons start when the parser exists.

## Limitations

- No public parser API yet (module 12): no default handler,
  `XML_StopParser`/`XML_ResumeParser`, unknown-encoding handler or
  `XML_GetInputContext`. External entity parsers exist inside
  `expat::parse` (module 11d) but not yet behind a public API.
- `Stop::DtdNotPorted` no longer exists (module 11d): every document
  parses to C's result; all 540 upstream vectors are compared under every
  option set.
- The allocation tracker replays C's allocations event by event (D-034);
  not modelled: `m_unknownEncodingMem` (no unknown-encoding handler yet)
  and the once-per-run base copy of `copyEntityTable` (bases are NULL
  without a base setter); the child encoding-name charge is untested. The
  input buffer is not tracked, as in C.
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
