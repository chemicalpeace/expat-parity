# Decisions

Append-only log. Each entry: id, date, decision, why, consequences.
Dates are 2026-10-11 unless stated otherwise.

## D-001 Target selection (Phase 0) — FROZEN

**Decision:** port **libexpat 2.9.0** (tag `R_2_9_0`, commit
`a76b1174c1ea52abb639a4c69d52b630a8a106a2`).

### Hard requirements

| Requirement | libexpat 2.9.0 |
|---|---|
| Permissive license | MIT (`expat/COPYING`) |
| Pure computation on untrusted input | XML 1.0 tokenizer + parser; no I/O, no network (external entities are delegated to the caller) |
| Reference source + tests | `expat/lib` + `expat/tests` (~18.3k lines of C tests), OSS-Fuzz fuzzers in `expat/fuzz` |
| 3k–25k lines of C | `expat/lib/*.c,*.h`: 18,494 lines (`wc -l`, including headers and comments) |
| Deterministic | Yes, given a fixed hash salt (`XML_SetHashSalt16Bytes`); the salt only seeds internal hash tables |

### Scoring (0–5 per criterion; judgment calls, crates.io data fetched 2026-10-11)

"Absence" = absence of a mature pure-Rust equivalent (5 = nothing comparable).

| Candidate | License | Lib size (C lines) | Security relevance | Deployment | Absence | Test quality | Modularity | **Total** |
|---|---|---|---|---|---|---|---|---|
| **libexpat 2.9.0** | MIT | ~18.5k | 5 — long CVE record incl. CVE-2022-25235/25236/25315, CVE-2024-45490/45491/45492, CVE-2024-8176, CVE-2025-59375 | 5 — CPython `xml.*`, Firefox, Apple/Android platforms, many distros | 4 — no expat-compatible safe port on crates.io; xml-rs / quick-xml / xmlparser are mature but differ in API and DTD/entity semantics | 4 — large C unit suite, W3C xmltest runner, OSS-Fuzz | 3 — clean tok/role/parse layering, but `xmlparse.c` is a 9.7k-line monolith | **21** |
| libyaml 0.2.5 | MIT | ~10k | 3 | 4 | 1 — `libyaml-safer` (safe port), `unsafe-libyaml`, `yaml-rust2` | 4 — yaml-test-suite | 4 | 16 |
| LZ4 1.10 | BSD-2 | ~8k | 3 | 5 | 1 — `lz4_flex` (150M downloads; no HC) | 3 | 4 | 16 |
| giflib 5.2 | MIT | ~5k | 4 | 4 | 1 — `gif` crate; `giflib-rs` port exists | 2 | 4 | 15 |
| libinjection 4 | BSD-3 | ~4k + generated data | 3 | 3 | 1 — `libinjectionrs`, `barbacane-libinjection` | 4 | 4 | 15 |
| bzip2 1.0.8 | bzip2 (BSD-style) | ~5k | 3 | 5 | 0 — `libbz2-rs-sys` (42M downloads), already the optional backend of `bzip2` 0.6 | 3 | 4 | 15 |
| md4c 0.5 | MIT | ~7k | 3 | 3 | 1 — pulldown-cmark, comrak | 3 | 3 | 13 |
| cJSON 1.7 | MIT | ~3.5k | 3 | 4 | 0 — serde_json | 2 | 4 | 13 |

**Why libexpat:** highest security relevance and footprint of the field, and the
only candidate where no comparable memory-safe drop-in exists. Its size is at the
top of the allowed range; we accept that and cut *scope order*, never
verification (see D-004).

## D-002 Build configuration of the port

Match upstream's default CMake configuration: `XML_DTD`, `XML_GE=1`, `XML_NS`,
`XML_CONTEXT_BYTES=1024`, `XML_Char` = UTF-8 `char`. `XML_UNICODE` /
`XML_UNICODE_WCHAR_T` builds (UTF-16 `XML_Char`) are **out of scope** for v0.x;
UTF-16 *input documents* are in scope.

## D-003 Repository and visibility

Repository `chemicalpeace/expat-rs`. Initially requested private per the owner's
standing privacy instruction; the owner then explicitly asked for this project
to be **public** ("bu projeyi Public yap"), which supersedes it for this
repository only.

**Blocker (2026-10-11):** the GitHub integration available to the agent lacks the
`repository_creation`/`administration` permission (`POST /user/repos` and the
GraphQL `createRepository` both return 403 "Resource not accessible by
integration"; installation uses *selected* repositories). The repository
therefore has to be created by the owner (public, empty) and added to the
integration's repository list; until then all work is committed locally and
pushed as soon as the remote exists. Recorded in `docs/STATE.md`.

## D-004 Scope order

Bottom-up, one module per iteration (see `docs/STATE.md` for the live list):
character tables → name predicates → UTF-8/16 encoders → SipHash →
tokenizer (`xmltok_impl.c`, UTF-8 / Latin-1 / ASCII / UTF-16LE/BE / unknown
encodings) → XML declaration and encoding detection → prolog state machine
(`xmlrole.c`) → parser core (`xmlparse.c`: pools, hash tables, content,
attributes, namespaces, DTD, entities, amplification limits) → public API →
optional C ABI. If time runs out, later modules are cut; already-ported modules
are never shipped with weaker verification.

## D-005 Crate layout

- `crates/expat` — the safe port, `#![no_std]` + `alloc` core with an `std`
  feature, `#![forbid(unsafe_code)]`.
- `crates/expat-oracle` — vendored C + shims, built with `cc`. **Test-only.**
  Contains `unsafe` FFI; every block carries a `SAFETY:` comment and
  `clippy::undocumented_unsafe_blocks` is denied. Never a dependency of
  `crates/expat`.
- `tests/` — the `expat-difftest` crate (harness, vectors, differential tests).
- `benches/` — the `expat-bench` crate (criterion).
- `fuzz/` — cargo-fuzz crate, excluded from the workspace (needs nightly).
- `crates/expat-capi` — deferred until the public API exists (D-004 order).

## D-006 Internal API for white-box differential tests

Bottom-up verification needs to compare internal modules before the public
parser exists. `crates/expat` exposes them only behind the non-default
`internal-api` feature as `expat::internal::*`, documented as unstable. The
default public API stays minimal.

## D-007 Toolchain

`rust-toolchain.toml` pins stable `1.99.0`. Fuzzing and Miri use nightly (CI
installs it explicitly); they never build release artifacts.

## D-008 Oracle configuration shim

Upstream generates `expat_config.h` with CMake. `crates/expat-oracle/shim/expat_config.h`
is a hand-written equivalent for D-002, with `/dev/urandom` as the entropy
source so the code compiles on Linux and macOS. The oracle always sets a fixed
16-byte hash salt, so entropy never affects output. Vendored files are
unmodified copies of `expat/lib/*` plus `COPYING` and `AUTHORS` (build-system
files `Makefile.am`, `*.cmake`, `libexpat.map.in` were not copied).

## D-009 White-box oracle shim

`shim/oracle_internals.c` textually includes the unmodified `xmltok.c` (with its
exported symbols renamed by `#define`) to reach `static` leaves such as the
byte-type tables and UTF-8 predicates. This is a build shim, not a change to
the vendored source.

## D-010 Definition of "equal"

The differential harness compares canonical *traces* (event stream + final
status, error code and position). The format and the equivalence rules
(coalesced character data, escaping) are specified in `docs/ARCHITECTURE.md`.
The trace is produced on the C side by `shim/oracle_trace.c` and on the Rust
side by `tests/src/trace.rs`; both must follow the same spec.

## D-011 Debt-check scope

`scripts/debt-check.sh` enforces the no-debt rules (no `todo!`,
`unimplemented!`, `TODO`/`FIXME`/`XXX`/`HACK`, no commented-out code
heuristics, no `unwrap()`/`expect(` in library code, every `#[allow` must carry
a `// reason:` justification on the same or previous line, no `unsafe` outside
`crates/expat-oracle` and `crates/expat-capi`). Test, bench and fuzz code may
use `expect("…")` because a panic is the assertion mechanism there.

## D-012 CI runs only after the remote exists

Until the remote exists (D-003) every CI job is executed locally with the same
commands as `.github/workflows/ci.yml` (`scripts/ci-local.sh`), and results are
recorded in `docs/STATE.md`. "CI green" in reports means *local* until the
first GitHub Actions run is visible.

## D-013 Remote exists; CI is GitHub Actions (supersedes the blocker in D-003 and D-012)

The owner created the public repository `chemicalpeace/expat-rs` and granted
the integration access. `main` was pushed and the first Actions run
(run 38147233705, commit `ebc8e89`) was green on every job: check, test on
`ubuntu-latest` and `macos-latest`, Miri, fuzz smoke. From now on "CI green"
means GitHub Actions. `scripts/ci-local.sh` stays as the local equivalent.

Workflow from here: every change goes to a short-lived branch, a pull request
is opened, and it is merged into `main` only after CI on the PR is green.
History is never rewritten (no force-push, no rebase of pushed commits).

## D-014 Tokenizer representation (module 5)

`xmltok_impl.c` is a macro template instantiated per encoding family. The
port is one set of generic functions (`expat::tok::scan`) over a trait
(`expat::tok::Encoding`) whose methods are the template's macros. Decisions:

- **Offsets, not pointers.** A scanner gets a slice and a start offset; the
  end of the slice is the C `end`.
- **`nextTokPtr` is `Scanned::next: Option<usize>`**, `None` exactly when C
  leaves `*nextTokPtr` untouched. This is stricter than the parser needs, so
  the differential tests also pin when C writes the pointer.
- **Typed results.** `Scan::{Token, PartialToken, Invalid, Partial, ...}`
  instead of a signed `int`; `Scan::to_c` restores the C value and every
  differential check compares through it.
- **Arithmetic lint.** `scan.rs` allows `clippy::arithmetic_side_effects`
  module-wide, with the bound written down: offsets never exceed
  `buf.len() + 4` and slices are at most `isize::MAX` long. Indexing is still
  denied (`get` only), so no lookup can panic.
- **Functions that trust their input** (`nameLength`, `skipS`, `getAtts`,
  `charRefNumber`) read in C until a terminator, out of bounds on input that
  breaks their precondition. The port additionally stops at the end of the
  slice. Equality is required, and tested, only on input that meets the C
  precondition (the token the scanner accepted). This is not a divergence of
  observable parser behaviour: the parser only calls them on such tokens.
- **Latin-1/ASCII name predicates.** C stores `NULL` for them because their
  tables contain no lead-byte types; the port uses the UTF-8 predicates for
  every single-byte encoding, which are equally never reached.

## D-015 Oracle access to the C tokenizer

`shim/oracle_internals.c` exposes the scanners and helpers through each
encoding's `ENCODING` vtable (`oracle_tok_*`), so the same entry points serve
the UTF-16 encodings of module 6. The Rust wrappers (`expat_oracle::tok`)
call the C functions that rely on a terminator on a copy of the buffer
followed by a padding that ends every one of them (four NUL characters, then
`>">'>` for `getAtts` or `;` for `charRefNumber`), so the wrappers are safe
to call on arbitrary input and AddressSanitizer stays meaningful under fuzz.

## D-016 Windows in the CI matrix; width of line/column counters

`windows-latest` (MSVC) joins the test matrix. The oracle builds there with
upstream's own Windows entropy source (`random_rand_s.c`, selected by
`build.rs`; `shim/expat_config.h` defines the POSIX-only macros only outside
`_WIN32`).

libexpat's `XML_Size` is `unsigned long` in the default configuration
(`XML_LARGE_SIZE` off): 64-bit on Linux/macOS, 32-bit on Windows. The port
uses `u64` line and column counters on every platform, i.e. it matches C on
LP64 targets and does not replicate the 32-bit wrap-around of the Windows
build. The difference is only observable after 2^32 lines or columns. The
differential tests compare the wrap-around near `u64::MAX` on LP64 targets
and use small start values on Windows.

## D-017 Sign of stray bytes in `utf8_toUtf16`

`utf8_toUtf16` copies a byte that does not start a multi-byte sequence with
`*to++ = *from++`, i.e. through C `char`. For bytes `>= 0x80` (continuation
or malformed bytes) the result is `0xFF80..=0xFFFF` where `char` is signed
(x86-64 Linux, macOS on arm64, Windows: every CI target) and `0x0080..=0x00FF`
where it is unsigned (aarch64 Linux). The port sign-extends on every
platform, matching C on all CI targets. Such bytes never reach the converter
from the parser (the tokenizer rejects them first), so no parse result
depends on it. On targets with unsigned `char` the differential harness
accepts the zero-extended C value for exactly these units of the UTF-8
encodings (`expat_difftest::tok::utf16_units_equal`); everywhere else the
comparison is exact.

## D-018 UTF-16 and unknown encodings (module 6)

- **One trait for the whole `ENCODING`.** `Encoding` gains `is_utf8`,
  `to_utf8` and `to_utf16`. The converters take an input and an output slice
  and return `Converted { result, read, written }`: `read`/`written` are how
  far C moves `*fromP`/`*toP`.
- **UTF-16** is `tok::Utf16 { Little, LittleNs, Big, BigNs }`. A code unit that
  is not complete in the buffer classifies as `BT_NONXML` (C would read past
  the end). `isUtf16` (a host byte-order optimisation for `XML_UNICODE`
  builds) and the internal UTF-16 encodings are not ported (D-002).
- **Unknown encodings** are `tok::Unknown<C: Converter>`, built by
  `Unknown::new(map, Option<C>, ns)`, which fails (`InvalidMap`) exactly when
  `XmlInitUnknownEncoding` returns `NULL`. C passes the converter a pointer
  and the converter decides how far to read; the port passes the slice of
  the sequence, whose length the map defines. A multi-byte sequence that is
  not complete in the input makes `to_utf8`/`to_utf16` stop with
  `InputIncomplete` (for `to_utf16`, a full output is reported first, like
  C's loop condition); C reads past the end there. The tokenizer only
  accepts complete sequences, so the parser never gives a converter such
  input. The differential tests compare with C only on input that ends on a
  sequence boundary and pin the port's behaviour on the rest separately.
- **Oracle access.** The oracle's tokenizer entry points take an `ENCODING *`
  (`oracle_encoding(id)` for the built-ins, now including `big2_`;
  `oracle_unknown_new` for unknown encodings, with a table-driven converter
  that the Rust harness mirrors). The 256-entry byte maps and 65,536-entry
  converter tables are generated from a 64-bit seed, so the fuzz target
  explores maps and converters together with documents.
- **Padding for unknown encodings.** The oracle wrappers of the helpers that
  read until a terminator (D-015) pad with `>` instead of NUL for unknown
  encodings, because the map may make NUL a lead byte but must map `>` to
  itself.

## D-019 Encoding detection and the XML declaration (module 7)

- **`InitEncoding`** is `INIT_ENCODING` after `XmlInitEncoding(NS)`: the index
  of the encoding the application named and the namespace flag. C stores the
  index in the `isUtf16` byte and makes the parser's encoding pointer point at
  the init encoding until `initScan` overwrites it; the port returns the
  selected encoding from `init_scan` as `InitScan::encoding` (`None` exactly
  when C leaves `*encPtr` at the init encoding) and the parser will hold
  either state explicitly.
- **`Known`** names a built-in encoding (`Normal` or `Utf16`) and dispatches
  `XmlTok` to the generic scanners; `Known::from_index` is `NS(encodings)`.
- **`parse_xml_decl`** returns every out-parameter of `XmlParseXmlDecl` as C
  leaves it, including those written before a failure, so the differential
  test can compare all of them. `findEncoding`'s result is `FoundEncoding`:
  `Input` for C returning the document's own encoding (`UTF-16` read as
  UTF-16), `Known`, or `Unrecognized` (`NULL`).
- **`toAscii`** returns the converted byte as a C `char` (sign-extended,
  D-017). Declaration tokens never convert to a byte where the sign matters
  (that would need a converted `0xFF`).
- **Precondition.** Like C, `parse_xml_decl` is only specified for a token the
  scanner returned as `XML_TOK_XML_DECL`; on other input C can move past `end`
  and read out of bounds, while the port stays inside the slice. The oracle
  wrapper asserts the shape C needs (at least `<?xml?>`, whole characters) and
  pads the buffer; the harness calls it on every declaration token a scanner
  accepts.
- **Q-3** is resolved as "replicate" (DIVERGENCES.md).

## D-020 Prolog state machine (module 8)

- **State as an enum.** C keeps `PROLOG_STATE::handler` as a pointer to one
  of 51 static handler functions; the port stores `role::Handler` and
  `PrologState::token_role` dispatches on it to one method per C handler.
  The other fields (`level`, `role_none`, `include_level`,
  `document_entity`, `in_entity_value`) are public, like the C struct the
  parser reads and writes.
- **Tokens.** The handlers take the scanner's `Scan`. Only complete tokens
  and `XML_TOK_NONE` are listed in any handler; every other value takes the
  default branch, exactly like the negative C values.
- **Uninitialised fields.** `XmlPrologStateInit` does not set `level` or
  `role_none`; the port sets them to 0 and `Role::None`. Both are always
  written before they are read (`element1` sets `level`, every transition to
  `declClose` sets `role_none`), so this is not observable; the oracle shim
  zeroes its state too, and the differential tests compare the complete
  state after every token.
- **Counters** `level` and `include_level` are `u32` with wrapping
  arithmetic, like C's `unsigned`; they are bounded by the number of `(` and
  `<![INCLUDE[` tokens.
- **Oracle access.** `shim/oracle_role.c` includes the unmodified
  `xmlrole.c` (its two exported functions renamed, D-009) and maps the
  handler pointer to its index in declaration order; `oracle_role_run` can
  start from any state, which lets the tests check every handler with every
  token kind and keyword in one step.

## D-021 String pool, hash table and allocation accounting (module 9)

- **Same structure as C.** `pool::StringPool` keeps `STRING_POOL`'s two block
  lists (as vectors, C list head last), the `int` block sizes, and the three
  growth strategies of `poolGrow` (reuse a free block, enlarge the newest
  block in place when the current string starts it, or allocate a new block
  of `max(1024, 2 x (end - start))`), plus `poolClear`'s list order. The
  hash table keeps `HASH_TABLE`'s slot layout. This is not cosmetic: the
  slot order is observable (the parser iterates `dtd->prefixes` and the
  general entities to build the context string of an external entity
  reference, `getContext`), and the exact allocation sizes are what the
  allocation tracker (`XML_SetAllocTrackerMaximumAmplification`) and a
  failing allocator see. Whether module 11 replicates the tracker's byte
  counts for all parser structures is decided there; the pool and table
  already report C's sizes.
- **Handles instead of pointers.** Pool strings are `PoolStr { block,
  start, len }` (block numbered from the oldest in use, which is stable
  because only the newest block changes), read with `StringPool::get`.
  Table entries live in an arena and are named by `EntryId` (insertion
  index, valid until `clear`); slots hold ids. Each entry owns its key (C
  points at a pool string that outlives the entry). A stale handle is a
  logic error but memory-safe.
- **Allocation accounting.** `heap::Heap` receives every `MALLOC`,
  `REALLOC` and `FREE` the C code would make, with C's size
  (`offsetof(BLOCK, s) + size` for blocks, `sizeof(NAMED *) x size` for
  slot arrays, the caller's `createSize` for entries); `false` makes the
  operation fail exactly where C's would, leaving C's state (for example a
  failed first table allocation sets `power` without slots; an insertion
  that fails after doubling keeps the doubled table). Sizes use the target's
  pointer width, so they equal C's on every platform. The port allocates
  with Rust's allocator; `heap::Unlimited` never refuses.
- **Hash width.** C truncates the 64-bit SipHash value to `unsigned long`
  (32 bits on Windows); the port uses `core::ffi::c_ulong`, so its slot
  layout equals C's on each platform. Unlike D-016 (counters), this is a
  free choice with no correctness cost, and it keeps `getContext` output
  identical to C everywhere.
- **Preconditions.** Keys and the strings given to `poolCopyString`,
  `poolCopyStringNoFinish` and `poolAppendString` are C strings (no NUL);
  C would stop at a NUL (and its `keyeq` can then read out of bounds). The
  port asserts this in debug builds; the parser only passes names.
- **C quirk kept:** `poolAppendString` of an empty string to a pool without
  blocks returns `NULL` (nothing was allocated, so `start` is `NULL`), which
  callers report as out of memory; the port returns `Err(NoMemory)`. The
  parser only appends non-empty constants, so this is not observable through
  the API. Found by the random differential programs.
- **Oracle access.** `shim/oracle_core.c` textually includes the unmodified
  `xmlparse.c`, every exported symbol renamed with `oracle_p_` (D-009), and
  interprets operation programs on a parser created with
  `XML_ParserCreate_MM` whose allocation hooks record and refuse requests
  (the allocation tracker is disabled). Runs are serialised by a lock in
  `expat-oracle` because the hooks use global state.

## D-022 Optimised dev/test builds of the port, harness and oracle

`Cargo.toml` sets `opt-level = 2` in the `dev` profile (which `cargo test`
uses) for `expat`, `expat-difftest` and `expat-oracle` (the C oracle is then
built with `-O2` too, as it already is for fuzzing). Debug assertions and
overflow checks are profile settings that stay on, so nothing that is checked
changes; the differential suites just run 10-20 times faster (whole
workspace 208 s -> 57 s including the build on the 2-core sandbox,
`tok_diff.rs` 163 s -> 8 s). Miri ignores `opt-level`. Taken in module 9
because each module adds heavy differential tests.

## D-023 Parser core without the DTD (module 10)

`expat::parse` ports the processors of `xmlparse.c` that a document
without a document type declaration reaches: `XML_Parse`,
`XML_GetBuffer`, `XML_ParseBuffer`, `callProcessor` with reparse deferral,
`prologInitProcessor`/`prologProcessor`/`doProlog` (roles without a DTD),
`processXmlDecl`, `contentProcessor`/`doContent`, `storeAtts`,
`addBinding`/`freeBindings`, `storeAttributeValue`/`appendAttributeValue`,
CDATA sections, `epilogProcessor`, the reports of PIs and comments, the
implicit `xml` binding (`setContext(implicitContext)`), the position and
byte-index getters, and the billion-laughs counters
(`accountingDiffTolerated`).

- **Handlers.** C behaves differently depending on whether a handler
  pointer is set (for example `m_eventPtr` after an empty element), so
  which handlers are set is parser configuration (`HandlerSet`) and the
  callbacks are one `Handler` object passed to `parse`. The start handler
  receives `XML_GetSpecifiedAttributeCount` as an argument. The default
  handler, `XML_StopParser`/`XML_ResumeParser`, external entity parsers and
  the unknown-encoding handler belong to the public API (module 12); the
  status checks after every token (`XML_SUSPENDED`, `XML_FINISHED`,
  `m_reenter`) are ported now and become reachable then.
- **Buffer exactly like C.** `m_buffer`/`m_bufferPtr`/`m_bufferEnd`/
  `m_bufferLim` are offsets into a `Vec<u8>` whose length is the C
  allocation; the first allocation (2 x 1024), the doubling, the 1024
  context bytes kept before the parse position, the in-place move and the
  overflow checks follow `XML_GetBuffer`. This is observable: the
  reparse-deferral heuristic reads the free space and
  `m_lastBufferRequestSize`, and decides when a partial token is scanned
  again (and with it chunk-dependent results such as Q-1/Q-2). Event,
  position and parse-end pointers are `Option<usize>` offsets (`None` =
  `NULL`).
- **Encodings.** `m_encoding` is `Enc::Init(InitEncoding)` until detection
  decides, then `Enc::Known`; the processors are generic over
  `tok::Encoding` and instantiated per family (`with_known!`). When the XML
  declaration switches the encoding, `doProlog` returns and the next token
  is scanned in the new encoding, which is what C's loop does.
- **Structures.** Element types, attribute ids and prefixes live in the
  module 9 hash tables (slot order is C's; it becomes observable through
  `getContext` with external entities); the `name[-1]` flag of an
  attribute id is a field; bindings are an arena with C's free list. Open
  tags keep a copy of their raw name, so `storeRawNames` has nothing to do.
  Expanded element names are computed when the tag starts; C rebuilds the
  same bytes at the end tag from the binding's URI, which cannot change
  while the tag is open. The duplicate check for prefixed attributes
  (`m_nsAtts`, a hash table with version stamps) is a list of
  (SipHash value, expanded name) pairs per start tag with C's comparison
  rule (same hash, and the stored name starts with the new one); entries
  with equal hashes share a probe sequence in C, so both find the same
  duplicates.
- **`errorProcessor`** returns `m_errorCode`. After an error, a later
  `XML_Parse` whose processing the reparse-deferral heuristic skips resets
  `m_errorCode` to `XML_ERROR_NONE`, and the next call then succeeds. The
  port does the same (found by `buffer_diff`, see REPORT).
- **Allocation accounting.** The parser does not announce allocations
  beyond those of the hash tables (with an unlimited heap). Without a
  failing allocator or an allocation-tracker limit nothing observable
  depends on them; whether the port replicates the tracker's byte counts
  for every parser structure is still decided in module 11 (D-021).
- **Salt.** `Parser::new` takes the 16-byte hash salt: the core crate is
  `no_std` without an entropy source. The public API (module 12) chooses
  where it comes from; the differential harness uses the oracle's fixed
  salt.
- **Scope boundary.** Any `doProlog` role that belongs to a document type
  declaration, and a declared general entity (which only a DTD can
  create), stops the parser with `Stop::DtdNotPorted`, never with a guessed
  libexpat error. The trace adapter reports such documents separately and
  every differential test counts them; module 11 removes the variant.
- **Oracle additions.** `oracle_options` gains `buffer_api` (each piece fed
  through `XML_GetBuffer(2n + 3)` + `XML_ParseBuffer(n)`, so the request
  size differs from the data) and `no_reparse_deferral`.
  `oracle_buffer_run` (`shim/oracle_core.c`, which includes `xmlparse.c`)
  feeds a document with an operation list and writes the buffer state
  after every operation; operations never parse bytes the caller did not
  write (comparing uninitialised C memory would be meaningless).

## D-024 Q-1 and Q-2: replicate (module 10)

Both quirks are in `epilogProcessor`: a CR at the end of the available
input is a partial `XML_TOK_PROLOG_S` that the epilog consumes, so its
line break is counted when the chunk ends there and again when an LF
follows in the next chunk (Q-1), and a final CR is consumed only when it
arrived in a non-final chunk (Q-2). Decision: **replicate**. XML 1.0
§2.11 governs the characters passed to the application, and libexpat
passes none for white space in the epilog in either case; error and
position reporting is not specified by XML 1.0. The difference is a
position after the root element, never an event, an error or a
well-formedness verdict, so there is no specification violation to fix,
and replicating keeps the port equal to C under every chunking.
Regression tests: `tests/tests/parse_diff.rs::q1_q2_epilog_line_breaks_replicate_c`
(the reproducers whole and in 1-byte chunks, compared with C and pinned to
C's values), `crates/expat/src/parse/tests.rs::epilog_cr_lf_split_counts_two_lines_like_c`,
and the chunk-dependence pin of the upstream vectors.

## D-025 Versioned fuzz corpora and a corpus replay test (module 10)

The committed corpus of every fuzz target is test data with a documented
origin (`docs/FUZZING.md`): a libFuzzer merge of the previous corpus, the
working corpus of the logged runs and deterministic seeds
(`expat_difftest::fuzzcase::seeds`, constant seed, `fuzz_seeds` binary).
What a fuzzing run discovers is not reproducible bit for bit (libFuzzer
draws a random seed per run), so the merged result is committed instead of
being regenerated in CI. The checks of the whole-parser targets live in
`expat_difftest::fuzzcase`, so the fuzz targets and
`tests/tests/fuzz_corpus.rs` run the same code; the replay test makes the
corpora part of the ordinary suite on Linux, macOS and Windows.
`scripts/fuzz-smoke.sh` fails for a target without a committed corpus
instead of creating an empty one. The raw libFuzzer logs of the runs
reported in REPORT.md are kept, trimmed, in `docs/fuzz-logs/`.

The harness gained `ParseOptions::ns_triplets` (`XML_SetReturnNSTriplet`)
after an injected bug in the triplet-dependent duplicate-attribute check
went undetected (REPORT.md, module 10).


## D-026 Security defaults of the DTD port: identical to C (module 11)

Specification: `docs/DTD-SECURITY.md` (written before the port, with
`xmlparse.c` line references). The port keeps every default and every
check of libexpat 2.9.0: parameter entity parsing `Never`, internal
entities expanded, billion-laughs protection with maximum amplification
100.0 and activation threshold 8 MiB, the same recursion, standalone and
`is_internal` checks at the same places, and the same error codes and
positions. The core crate has no I/O; external entities are read only by
a caller's handler (module 12), so the library itself never fetches a
resource, like C.

Stricter defaults were evaluated (no internal entity expansion, a lower
amplification limit, a cap on entity nesting or replacement text size)
and rejected as defaults: each changes the result for documents C
accepts, which would be a divergence without a specification bug to
justify it. Callers can already tighten the amplification limits through
the same setters as in C (opt-in). No security check may be relaxed to
make a differential test pass; a mismatch is fixed at its root cause or
recorded in DIVERGENCES.md.

## D-027 Declarations: data structures (module 11a)

- **Entities.** `ENTITY` is `dtd::Entity` in the module 9 hash tables
  `general_entities` and `param_entities` (slot order is C's; it becomes
  observable through `getContext`). The replacement text is an
  `Arc<[u8]>` so the expansion (11b) can scan it while the parser state
  changes; `Arc` rather than `Rc` keeps the parser `Send`.
  "Created by this lookup" is C's `entity->name == name` test.
- **Content models.** The scaffold mirrors `CONTENT_SCAFFOLD` (reused
  between declarations like C's array) and `build_model` produces a flat
  node array in C's order (children contiguous), so neither building,
  printing nor dropping a model recurses; models nested 20,000 deep are
  compared with C.
- **`defaultAttForName`** is a `BTreeMap` from attribute id to the index
  of the first definition: C's table is private and its slot order is
  never observable; lookups are the same. `storeAtts` uses it exactly where
  C does (found by an injected bug, REPORT.md).
- **Temporary pool contents** (`m_doctypeName`, `m_declAttributeType`,
  notation names) are owned `Vec`s. C's `poolClear` leaves dangling but
  non-`NULL` pointers that are only tested, never read (for example
  `m_doctypeSysid` after the internal subset starts); the port keeps the
  `Option` set in those places so the same tests give the same result.
- **Scope boundary (11a).** Expanding a declared internal entity
  (content, attribute value, parameter entity in the prolog) and the roles
  that only external parameter entities produce (text declaration,
  conditional sections, inner PE references) still return
  `Stop::DtdNotPorted`; the not-standalone and external-entity-reference
  handlers belong to the public API (module 12) and behave as unset, like
  C without them.

## D-028 Entity expansion: event pointers and the free-entity count (module 11b)

- **Event pointer of an open entity.** C decides whether `doContent`,
  `doCdataSection` and `doProlog` write `m_eventPtr`/`m_eventEndPtr` or the
  `internalEventPtr`/`internalEventEndPtr` of the innermost open internal
  entity by comparing `enc != parser->m_encoding` (`xmlparse.c` 3587-3593,
  5524-5530). The port has no encoding pointers; it asks the encoding
  (`tok::Encoding::is_internal`, true only for the internal UTF-8
  encodings that replacement text is scanned with). `m_encoding` is never
  one of them (`XmlGetInternalEncoding` returns distinct structures in C),
  so the test is equivalent. The guarded writes of `storeAtts`,
  `appendAttributeValue` and `storeEntityValue`
  (`if (enc == parser->m_encoding) parser->m_eventPtr = ...`) are
  `Parser::ev_doc`. Effect: an error inside an expansion reports the
  position of the outermost reference in the document, like C (verified
  by the curated cases; the injected bug I4 in REPORT.md breaks 254 pairs).
- **Open-entity lists** are three `Vec<OpenEntity>` (innermost last)
  instead of C's linked lists; only the head is ever accessed.
- **`m_freeEntities`** (C's free list of `OPEN_INTERNAL_ENTITY` records
  reused by `processEntity`) is kept as a count only (`free_entities`).
  Nothing observable reads the list except through allocation: reusing a
  record avoids a `MALLOC` of `sizeof(OPEN_INTERNAL_ENTITY)`, which matters
  for the allocation tracker (11c). The count lets the tracker charge
  exactly the allocations C makes.

## D-029 Billion-laughs limits: setters, bounded tests, Q-4 (module 11b2)

- **Setters.** `Parser::set_billion_laughs_maximum_amplification` and
  `set_billion_laughs_activation_threshold` mirror
  `XML_SetBillionLaughsAttackProtection*` (`xmlparse.c` 3099-3148): NaN
  and values below 1.0 are refused, any threshold is accepted, both may be
  called at any time. The refusal on non-root parsers needs external
  entity parsers (11d / module 12) and is tested there.
- **Defaults stay C's** (100.0, 8 MiB; D-026). No limit is relaxed for a
  test: the tests set limits only through the same setters as C.
- **Harness options.** The oracle gained `limits` (all four setters; the
  allocation tracker pair is used from 11c), `trace_accounting` (the
  direct/indirect counters are compared, not only outcomes),
  `text_digest` (character data as length + FNV-1a 64, so traces of
  multi-megabyte expansions stay small) and `heap_cap` (capped
  `XML_ParserCreate_MM` allocator). The fuzz target's second selector byte
  uses bit 7 for small limits (factor 1-4, threshold 0-480 bytes) with the
  counters traced.
- **Bounded tests.** Every amplification comparison is bounded in input
  size, wall-clock time and memory (`expat_difftest::bounded`; table in
  DTD-SECURITY.md). The counting allocator needs `unsafe`, so it lives in
  `expat-oracle` (D-011).
- **Q-4.** libexpat's `epilogProcessor` adds `NULL - s` to the direct
  counter for a name at the end of the buffer (undefined behaviour; the
  parse ends with error 9 regardless). The port counts 0 bytes for that
  token; the harness masks the counter line only in that exact situation
  (DIVERGENCES.md Q-4). Replicating an address-dependent value is
  impossible and would not be meaningful.

## D-030 Allocation tracker: charge at least C's bytes, no later (module 11c)

- **What C does.** libexpat 2.9.0 counts every `MALLOC`/`REALLOC` of a
  parser (`expat_malloc` 898-950, `expat_realloc` 987-1060) as
  `sizeof(size_t) + EXPAT_MALLOC_PADDING + size` and refuses an allocation
  (`XML_ERROR_NO_MEMORY`) when the new total is at or above the activation
  threshold (64 MiB) and `total / countBytesDirect` exceeds the maximum
  amplification (100.0) (`expat_heap_increase_tolerable` 860-894). Two
  allocations are deliberately outside the tracker: the input buffer
  (`XML_GetBuffer`, plain `malloc_fcn`, "to leave limiting the input size
  to the application", 2601-2603) and the content models given to the
  element declaration handler (6442, 8780).
- **Decision: charge, do not replicate byte for byte.** The port keeps its
  own data structures (owned `Vec`s, D-021, D-027), so it cannot count C's
  allocations. It *charges* its tracker (`parse::tracker`) at the point
  where C allocates, or earlier, at least the bytes C allocates:
  exactly C's sizes for the structures whose growth the port replays
  (string pools through `PoolShadow`, which replays `poolGrow` on lengths
  only; hash tables through `heap::Heap`; `TAG` structures, their name
  buffers and the free list; namespace bindings and their URI buffers;
  `m_atts`/`m_nsAtts`; open-entity records with C's free list (D-028);
  default-attribute arrays and their per-element tables; the content-model
  scaffold and group connector); and an upper bound,
  `CREATION_CHARGE` = 16 KiB, for the parser creation (3,040-3,059 bytes in
  C on 64-bit targets, checked on every CI platform by
  `creation_charge_covers_c`) plus the small fixed allocations the port
  does not replay one by one. Like C, the port does **not** charge the
  input buffer; charging it would make the port refuse large documents
  that C accepts at the default limits, which D-026 rules out.
- **Why.** Byte-exact replication would tie the port to C's structure
  layouts (`sizeof` per platform) and to every incidental allocation,
  for no security gain. The property that matters is one-sided: whenever
  libexpat refuses through its tracker, the port must refuse too. Charging
  at least as much, no later, gives it, because both sides count the same
  direct bytes (`parse::accounting`), so the port's ratio is never lower
  at any point of the parse.
- **Consequence (divergence DV-1).** The port's total is above C's by a
  margin (measured 11,764-13,352 bytes in the tests, dominated by the
  creation bound). A document whose C total ends within that margin below
  the refusal point is refused by the port only. With limits tightened by
  the caller this is easy to reach (at C's exact passing threshold the
  port refuses in 36/36 sweep cases); at the default limits it needs a
  tracked total of at least 64 MiB within ~13 KB of 100 times the direct
  input, and no test or fuzz input has reached it. It is fail-closed (the
  error is `XML_ERROR_NO_MEMORY`, as from C's tracker) and recorded in
  DIVERGENCES.md. This is the one exception to "no stricter behaviour"
  in D-026, accepted because the alternative (byte-exact replication) is
  not achievable with the port's data structures.
- **Verification.** `tests/tests/tracker_diff.rs` (groups in REPORT.md,
  module 11c), the tracker relation in the `parse_trace` fuzz target
  (`expat_difftest::tracker::check_from`), injected bugs K1-K7.

## D-031 External entities through test handlers (module 11d)

- **What C does.** libexpat never reads an external entity itself. It
  calls `XML_ExternalEntityRefHandler` with `getContext` (general
  entities) or a NULL context (parameter entities, the external subset),
  base, system and public identifier; the application may create an
  external entity parser and feed it. Returning `XML_STATUS_ERROR` stops
  the parse with `XML_ERROR_EXTERNAL_ENTITY_HANDLING` (21).
- **Decision.** The differential harness uses test handlers on both sides
  with identical, deterministic behaviour (trace the arguments, then fail
  or accept). In step D2 the accepting handler additionally reads the
  entity from an in-memory map keyed by system identifier, through an
  external entity parser; no file or network access exists in the
  library, the oracle shim or the harness. The port's `Handler` gets
  `external_entity_ref` and `not_standalone`, refusing by default.
- **Why.** Equal handler calls and arguments are the observable contract
  of this part of libexpat; an in-memory resolver keeps the tests
  hermetic and bounded.
