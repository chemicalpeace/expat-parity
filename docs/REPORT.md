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

## Divergences

`docs/DIVERGENCES.md`. One divergence: DV-1 (module 11c, D-030), the
allocation tracker may refuse (`XML_ERROR_NO_MEMORY`) slightly earlier
than C, within the margin by which its charge exceeds C's count; it is
fail-closed and was observed only with caller-tightened limits. No other
divergence in parse results. Four C quirks found
by the harness: Q-3 resolved as "replicate" (module 7; the input violates
XML 1.0 §4.3.3), Q-1 and Q-2 resolved as "replicate" (module 10, D-024),
Q-4 (module 11b2, D-029: undefined behaviour in C's epilog accounting that
corrupts only an internal counter) not replicated, masked in the harness.
Platform-dependent C behaviour fixed to one choice: D-016 (counter width),
D-017 (sign of stray bytes in `utf8_toUtf16`).

## Benchmarks

Only an oracle baseline exists (`cargo bench -p expat-bench`); port-vs-C
comparisons start when the parser exists.

## Limitations

- No public parser API yet (module 12): no default handler,
  `XML_StopParser`/`XML_ResumeParser`, external entity parsers,
  unknown-encoding handler or `XML_GetInputContext`.
- After module 11b the port stops with `Stop::DtdNotPorted` (never a
  guessed error) only where an external entity parser would be needed
  (external subset or external parameter entity with a handler); all 540
  upstream vectors are compared under every option set.
- The allocation tracker is not byte-identical to C: the port charges at
  least C's bytes, no later (D-030), so it can refuse slightly earlier
  (DV-1). The input buffer is not tracked, as in C.
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
