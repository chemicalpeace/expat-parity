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
| `siphash.h` | `expat::siphash` | planned |
| `xmltok_impl.c`, `xmltok.c`, `xmltok_ns.c` | `expat::tok` | planned |
| `xmlrole.c` | `expat::role` | planned |
| `xmlparse.c` | `expat::parse` (+ submodules) | planned |

Internal modules are reachable from tests only through the `internal-api`
feature (`expat::internal`), see DECISIONS D-006.

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
size, followed by an empty final call), parameter-entity-parsing mode. The C
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

## Safety and resource limits

- No `unsafe` in `crates/expat`; no global or static mutable state.
- All input-dependent indexing goes through checked accessors; clippy
  `indexing_slicing` and `arithmetic_side_effects` are denied in CI.
- libexpat's billion-laughs and allocation-amplification protections are part
  of the port's observable behaviour (error codes) and will be ported, not
  replaced.
