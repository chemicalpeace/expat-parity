# Architecture

## Goal

A memory-safe Rust implementation of libexpat 2.9.0 whose observable behaviour
is *equal* (defined below) to the C library on every input, except for
documented divergences (`docs/DIVERGENCES.md`).

## Crates

```
crates/expat          safe port (#![no_std] + alloc, #![forbid(unsafe_code)])
crates/expat-oracle   vendored libexpat C + shims, FFI — test-only
tests/                expat-difftest: harness, vectors, differential tests
benches/              expat-bench: criterion benchmarks (port vs C)
fuzz/                 cargo-fuzz targets (nightly, outside the workspace)
```

Dependency direction: `expat-difftest`, `expat-bench` and `fuzz` depend on both
`expat` and `expat-oracle`; `expat` depends on nothing.

## Module map (C → Rust)

| C source | Rust module | Status |
|---|---|---|
| `asciitab.h`, `iasciitab.h`, `latin1tab.h`, `utf8tab.h`, `xmltok_impl.h` (BT_*) | `expat::chartab` | see STATE.md |
| `nametab.h`, `UCS2_GET_NAMING`, `UTF8_GET_NAMING2/3`, `UTF8_INVALID2/3/4` (`xmltok.c`) | `expat::naming` | see STATE.md |
| `XmlUtf8Encode`, `XmlUtf16Encode` (`xmltok.c`) | `expat::encode` | see STATE.md |
| `siphash.h` | `expat::siphash` | see STATE.md |
| `xmltok_impl.c` (scanner template); normal encodings of `xmltok.c` | `expat::tok` (`scan`, `normal`) | see STATE.md |
| UTF-16 and unknown encodings, `utf8Convert`/`utf16Convert` converters (`xmltok.c`) | `expat::tok` (`utf16`, `unknown`, `convert`) | see STATE.md |
| `XmlInitEncoding(NS)`, `initScan`, `XmlParseXmlDecl(NS)`, encoding tables (`xmltok.c`, `xmltok_ns.c`) | `expat::tok` (`init`, `xmldecl`) | see STATE.md |
| `xmlrole.c` | `expat::role` | see STATE.md |
| `STRING_POOL`, `pool*` (`xmlparse.c`) | `expat::pool` | see STATE.md |
| `HASH_TABLE`, `lookup`, `hashTable*`, `hash.h`/`hash_table.h` (`xmlparse.c`) | `expat::hash` | see STATE.md |
| `MALLOC`/`REALLOC`/`FREE` accounting (`expat_malloc` & co., `xmlparse.c`) | `expat::heap` | see STATE.md |
| `XML_Parse`, `XML_GetBuffer`, `XML_ParseBuffer`, `callProcessor`, prolog without DTD, `processXmlDecl`, content, `storeAtts`, namespaces, CDATA, epilog, position getters, accounting (`xmlparse.c`) | `expat::parse` (`mod`, `prolog`, `content`, `dtd`, `accounting`, `error`) | see STATE.md |
| `doProlog` DTD roles, `storeEntityValue`, `defineAttribute`, `getElementType`, `nextScaffoldPart`, `build_model`, `normalizePublicId` (`xmlparse.c`) | `expat::parse` (`decl`, `model`, `dtd`) | module 11a |
| `processEntity`, `internalEntityProcessor`, `storeAttributeValue`, `callStoreEntityValue`, the open-entity lists | `expat::parse` (`entity`) | module 11b |
| External entity parsers, external subset (`XML_ExternalEntityParserCreate`, `externalEntityInitProcessor*`) | `expat::parse` | module 11d / 12 |

Internal modules are reachable from tests only through the `internal-api`
feature (`expat::internal`), see DECISIONS D-006.

## Tokenizer

`expat::tok::scan` ports `xmltok_impl.c` function by function, generic over
`expat::tok::Encoding` (the template's macros: `MINBPC`, `BYTE_TYPE`,
`BYTE_TO_ASCII`, `CHAR_MATCHES`, `IS_NAME_CHAR`, ...). `expat::tok::Normal`
implements it for the eight single-byte `normal_encoding`s of `xmltok.c`.
Representation choices (offsets, `Scanned::next`, typed `Scan`) and the
bounded variants of the precondition-trusting helpers are recorded in
DECISIONS D-014. The C case macros become helper methods returning a `Step`
(advance or return), so each Rust `match` lists the same cases as the C
`switch`.

`expat::tok::Utf16` implements it for the four UTF-16 encodings
(`little2_`/`big2_`, `MINBPC` 2) and `expat::tok::Unknown<C>` for encodings
built from an application map (`XmlInitUnknownEncoding(NS)`), where `C` is a
`Converter` that receives exactly the bytes of one multi-byte sequence. The
same trait carries the converters (`to_utf8`/`to_utf16`, the C
`utf8Convert`/`utf16Convert`), which return a `Converted { result, read,
written }` instead of moving two pointers (`expat::tok::convert`). See
DECISIONS D-017 and D-018.

Encoding detection (`expat::tok::init`: `InitEncoding`, `init_scan`, `Known`)
and the XML/text declaration (`expat::tok::xmldecl::parse_xml_decl`) complete
the port of `xmltok.c`/`xmltok_ns.c` (D-019). `tests/tests/init_diff.rs`
compares `initScan` for all strings of up to 4 bytes over the bytes it
dispatches on, for 11 encoding names x both states x both namespace modes,
and every declaration token that any scanner accepts is parsed by both
implementations with every out-parameter compared (generated declarations in
all 12 built-in encodings and in seeded unknown encodings; upstream vectors;
fuzzing).

Verification (`tests/tests/tok_diff.rs`, `expat_difftest::tok`, fuzz target
`tok_diff`): every scanner is compared with C on token *and* `next` (including
whether C writes it) for all strings of up to 3 bytes over a 56-byte alphabet
that covers every byte type (single-byte encodings) and of up to 3 code units
over a 41-unit alphabet (UTF-16, also with a dangling byte and from an odd
offset), at every offset of the upstream vectors (also transcoded to UTF-16),
on token-by-token walks of every vector, and on random XML-ish fragments.
Unknown encodings are generated from a seed (map and converter table, the
same on both sides; the oracle's table converter is mirrored by
`expat_difftest::tok::TableConverter`), including maps that C rejects. For
each accepted token the helpers whose precondition it establishes are
compared too; the total helpers are compared at every offset. The converters
are compared exhaustively on short inputs with every output size from 0 to 9
plus partial and ample sizes, on whole documents and on every content token
of a walk.

## Prolog state machine

`expat::role` ports `xmlrole.c`: `PrologState::token_role` gives each prolog
or DTD token its `Role` and moves to the next `Handler` (D-020).
Verification (`tests/tests/role_diff.rs`, `expat_difftest::role`, fuzz target
`role_diff`): the role and the complete state are compared with C after every
token, for (a) every handler x every token kind x 30 keyword texts from every
combination of the counters, flags and `role_none` (one step each, in UTF-8
and UTF-16BE), (b) the upstream vectors and a DTD that uses every
declaration form and conditional sections, tokenized with the prolog
scanner, as a document entity and as an external entity, in all 12 built-in
encodings, and (c) random synthetic token sequences (also in seeded unknown
encodings).

## Parser core data structures

`expat::pool` (`STRING_POOL`) and `expat::hash` (`HASH_TABLE`) keep the C
structure exactly (D-021): the pool's block lists, block sizes and growth
choices, and the hash table's slot layout (SipHash with the root parser's
salt, double hashing with the 8-bit `PROBE_STEP`, doubling at half full).
Allocations the C code would make are announced, with C's sizes, to an
`expat::heap::Heap`, which may refuse them like a failing `MALLOC`. Strings
are `PoolStr` handles and table entries `EntryId`s instead of pointers.
Verification (`tests/tests/core_diff.rs`, `expat_difftest::core`, fuzz target
`core_diff`): programs of pool / hash table operations run on the real C
structures (`shim/oracle_core.c`, which includes `xmlparse.c`) and on the
port; after every operation the result, every allocation request and the
complete white-box state are compared (block lists, `start`/`ptr`, the
current string, all finished strings; `power`/`size`/`used`, entry ids and
slots, iteration order, the final slot layout), including a refusal of
every single allocation attempt.

## Parser core (module 10)

`expat::parse::Parser` holds the parser state of `xmlparse.c` and runs its
processors (D-023): the input buffer is managed exactly like C's (sizes,
context bytes, reparse deferral), C pointers are offsets, the processors are
generic over the encoding and instantiated per family, events go to a
`Handler` and which handlers are set is configuration (`HandlerSet`).
Element types, attribute ids and prefixes use the module 9 hash tables;
bindings are an arena. A document type declaration stops the parser with
`Stop::DtdNotPorted` until module 11.

Verification: the trace adapter (`tests/src/port.rs`) drives the port with
the handlers of the C shim and writes the canonical trace, so whole parses
are compared with C (`tests/tests/parse_diff.rs`: the 540 upstream vectors
under 10 option sets, 2,500 generated documents x 3 random option sets,
the Q-1/Q-2 reproducers); `tests/tests/buffer_diff.rs` compares the buffer
state after every `XML_Parse` / `XML_GetBuffer` / `XML_ParseBuffer` call
white-box (`oracle_buffer_run`); fuzz targets `parse_trace` (differential)
and `buffer_diff`.

## Verification layers

1. **White-box module diffs.** Each leaf module is compared exhaustively (where
   the domain is small: all 256 bytes, all 65,536 BMP code units, all 2/3/4-byte
   sequences in the relevant lead-byte ranges) or with proptest against the C
   static functions reached via `shim/oracle_internals.c`.
2. **Black-box trace diffs.** Whole documents are parsed by both
   implementations under several option sets and chunkings; the canonical
   traces must be byte-equal.
3. **Upstream vectors.** XML documents extracted from libexpat's own test
   suite (`tests/vectors/`, provenance per file) run through layer 2, plus the
   expectations those upstream tests assert.
4. **Fuzzing.** One cargo-fuzz target per public entry point; each target runs
   the C oracle and the port on the same input and asserts trace equality (and
   that neither panics/crashes). Until a Rust entry point exists, a target
   exercises the oracle alone so corpus and infrastructure are already proven.

## Definition of "equal"

Two parses are equal iff their **canonical traces** are byte-identical.

The trace is produced for a given *(document bytes, options)* pair. Options:
encoding override (or autodetect), namespace separator (or none), chunk size
(0 = one `XML_Parse` call; otherwise the document is fed in chunks of that
size, followed by an empty final call), parameter-entity-parsing mode,
whether each piece goes through `XML_Parse` or through `XML_GetBuffer(2n + 3)`
+ `XML_ParseBuffer(n)`, and whether reparse deferral is disabled. The C
side always uses a fixed hash salt.

### Trace format

UTF-8/ASCII text, one event per line, fields separated by one space, each line
terminated by `\n`. String fields are escaped: every byte `<= 0x20`, `>= 0x7F`,
or one of `% ~ ( ) ,` is written as `%HH` (upper-case hex); all other bytes are
literal. A C `NULL` string is written as `~`. Integers are decimal.

| Line | Emitted for (libexpat handler) |
|---|---|
| `xmldecl <version> <encoding> <standalone>` | `XML_XmlDeclHandler` (standalone ∈ -1,0,1) |
| `doctype <name> <sysid> <pubid> <has_internal_subset>` | `XML_StartDoctypeDeclHandler` |
| `enddoctype` | `XML_EndDoctypeDeclHandler` |
| `start <name> <specified_attr_count>` | `XML_StartElementHandler`; count from `XML_GetSpecifiedAttributeCount` |
| `attr <name> <value>` | one line per attribute of the preceding `start`, in libexpat's order (specified first, then defaulted) |
| `end <name>` | `XML_EndElementHandler` |
| `text <data>` | `XML_CharacterDataHandler`, **coalesced**: consecutive calls are concatenated into one line, flushed by any other event or the end of the parse |
| `pi <target> <data>` | `XML_ProcessingInstructionHandler` |
| `comment <data>` | `XML_CommentHandler` |
| `cdata` / `endcdata` | `XML_StartCdataSectionHandler` / `XML_EndCdataSectionHandler` |
| `entity <name> <is_param> <value> <base> <sysid> <pubid> <notation>` | `XML_EntityDeclHandler` (`value` uses the explicit length; `~` for external entities) |
| `notation <name> <base> <sysid> <pubid>` | `XML_NotationDeclHandler` |
| `element <name> <model>` | `XML_ElementDeclHandler`; model = `<type><quant> <name>(<child>,<child>…)` recursively, type/quant as the numeric `XML_Content_Type`/`XML_Content_Quant` |
| `attlist <elname> <attname> <type> <default> <isrequired>` | `XML_AttlistDeclHandler` |
| `nsstart <prefix> <uri>` / `nsend <prefix>` | namespace declaration handlers (only with a separator) |
| `skipped <name> <is_param>` | `XML_SkippedEntityHandler` |
| `limit <name> <0\|1>` | only with `limits` set (module 11): first lines, one per setter called, in the order `bl-amplification`, `bl-threshold`, `at-amplification`, `at-threshold`, with the setter's `XML_Bool` |
| `textdigest <len> <fnv1a64>` | instead of `text` when `text_digest` is set: length and FNV-1a 64 (16 lower-case hex digits) of the coalesced character data |
| `accounting <direct> <indirect>` | only with `trace_accounting`: the billion-laughs counters of the root parser (`testingAccountingGetCountBytesDirect`/`Indirect`) after parsing, just before `result` (Q-4 in DIVERGENCES.md masks it in one known C defect) |
| `result <ok\|error> <error_code> <line> <column> <byte_index>` | always last: return status of the last `XML_Parse` call, `XML_GetErrorCode`, `XML_GetCurrentLineNumber64`, `XML_GetCurrentColumnNumber64`, `XML_GetCurrentByteIndex64` after parsing stops |

### Why these rules

- **Coalescing character data**: libexpat may split character data at buffer
  and chunk boundaries; applications cannot rely on the split, so it is not
  part of the observable contract. Everything else is compared exactly.
- **Error position** is part of the contract (applications report it), so it is
  compared exactly, including byte index.
- **Handlers not traced** (default handler, external entity loading, unknown
  encoding handler, `XML_GetInputContext`) change parser behaviour or expose
  buffer internals; they will get dedicated differential tests when the
  corresponding API is ported.

The C implementation of this spec is `crates/expat-oracle/shim/oracle_trace.c`;
the Rust implementation is `tests/src/trace.rs`. Both are tested against
hand-written expected traces.

## Entity expansion (module 11b)

Expansion is iterative, as in C since 2.7.0: a reference pushes an
`OpenEntity` on one of three lists (content and parameter entities,
attribute values, entity values) and returns; the replacement text is
consumed by the loop that owns the list (`Processor::InternalEntity` in
the processor loop for content and PEs; the loops of
`store_attribute_value` and `call_store_entity_value`). Nesting depth costs
one `OpenEntity` of heap per level and no stack. `Entity::open` stays set
until the entity and everything opened inside it are closed, which is how
direct and indirect recursion are detected. Event pointers inside an
expansion belong to the open entity (D-028). Verification:
`tests/tests/entity_diff.rs` (REPORT.md, module 11b).

## Safety and resource limits

- No `unsafe` in `crates/expat`; no global or static mutable state.
- All input-dependent indexing goes through checked accessors; clippy
  `indexing_slicing` and `arithmetic_side_effects` are denied in CI.
- libexpat's billion-laughs and allocation-amplification protections are part
  of the port's observable behaviour (error codes) and will be ported, not
  replaced.
