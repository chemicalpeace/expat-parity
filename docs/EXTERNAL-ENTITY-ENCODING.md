# External parsed entities: encoding (libexpat 2.9.0 vs expat-rs)

Scope: how the encoding of an external parsed entity is chosen when it is
read through an external entity parser (`XML_ExternalEntityParserCreate`),
how external entity processing is switched on and off, and how errors of
the child parser reach the parent. Every statement about C below is backed
by the vendored source (`crates/expat-oracle/vendor/expat/lib`, line
numbers of 2.9.0) and by the differential test
`tests/tests/ext_encoding_diff.rs`, which runs each case on the C oracle
and on the port and records both results separately. **Result: no
difference between C and the port in any case** (counts in
"Verification"). No library code was changed for this document.

## 1. How external entity processing is enabled

libexpat never opens a file or URL. An external entity is processed only if
the application installs `XML_SetExternalEntityRefHandler`
(`xmlparse.c` 2169); the handler may create a child parser with
`XML_ExternalEntityParserCreate(parser, context, encodingName)`
(1680-1863), feed it the entity's bytes and free it. The port has the same
shape: `Handler::external_entity_ref` (`crates/expat/src/parse/mod.rs`
207-216, refuses by default) is called only when
`HandlerSet::external_entity_ref` is set, and receives an
`ExternalEntities` factory whose `create(context, encoding)`
(`parse/external.rs` 88) is `XML_ExternalEntityParserCreate`
(`external_entity_parser_create`, `parse/external.rs` 317).

When the handler is called (C; the port's call sites are
`parse/content.rs` 410, `parse/decl.rs` 257 / 834 / 925,
`parse/prolog.rs` 266, verified equal by `configuration_matrix`):

| Entity | C call site | Called when | Context argument |
|---|---|---|---|
| External general entity referenced in content | `doContent` 3726-3740 | a handler is set (independent of parameter entity parsing and of `standalone`) | `getContext` string |
| External DTD subset (`<!DOCTYPE d SYSTEM ...>`) | `doProlog` 5702-5726 (end of DOCTYPE) | handler set **and** `m_paramEntityParsing != XML_PARAM_ENTITY_PARSING_NEVER` | NULL |
| Foreign DTD (`XML_UseForeignDTD`, 1951) | `doProlog` 5702 / 5760-5776 | as the subset; system id is NULL | NULL |
| External parameter entity between declarations (`%p;`) | `doProlog` 6383-6396 | handler set and parameter entity parsing not NEVER | NULL |
| External parameter entity inside an entity value | `storeEntityValue` 7187-7198 | as above | NULL |

Disabled:

- No handler: nothing is read. A general entity reference is passed to the
  default handler only; the subset and parameter entities are not loaded
  and, because the document has an external subset / PE references, an
  undefined entity in content is reported as skipped (trace `skipped`)
  instead of failing.
- `XML_SetParamEntityParsing(parser, XML_PARAM_ENTITY_PARSING_NEVER)`
  (2226, the default): the subset and parameter entities are not read; the
  handler is still called for general entities.
- `XML_PARAM_ENTITY_PARSING_UNLESS_STANDALONE`: becomes NEVER when the
  document's XML declaration says `standalone="yes"` (`processXmlDecl`
  5151-5157), so with `standalone="yes"` the subset/PEs are not read; with
  `ALWAYS` they are read, and a general entity declared there and used in
  a standalone document is `XML_ERROR_ENTITY_DECLARED_IN_PE` (24); without
  reading it, `XML_ERROR_UNDEFINED_ENTITY` (11).
- After the subset was read, the not-standalone handler is consulted for a
  document that is not standalone (5727-5740); returning 0 is
  `XML_ERROR_NOT_STANDALONE` (22).

Handler return value: 0 (`XML_STATUS_ERROR`) stops the parent with
`XML_ERROR_EXTERNAL_ENTITY_HANDLING` (21) at the reference (3739, 5726,
5776, 6396, 7198); any other value continues. The child's own error is
**not** copied to the parent: the parent only sees 21 (or continues if the
handler ignores the child's failure).

### Test handlers (identical on both sides)

The C shim (`crates/expat-oracle/shim/oracle_trace.c`, `on_extref`,
`read_external`) and the port adapter (`tests/src/port.rs`, `Tracer`)
implement the same four modes of `ParseOptions::ext_handler`:

| Mode | Handler set | Return value | Reads |
|---|---|---|---|
| `None` | no | — | nothing |
| `Fail` | yes, traces `extref` | `XML_STATUS_ERROR` | nothing |
| `Accept` | yes, traces `extref` | `XML_STATUS_OK` | nothing |
| `Read` | yes, traces `extref` | the child's status (`extend <ok> <code> <line> <column> <byte index>`); `XML_STATUS_OK` for an unknown or NULL system id | the in-memory body of `ParseOptions::ext_entities`, through a child created with `ParseOptions::ext_encoding` as `encodingName`, fed like the document (whole, chunks, buffer API); nesting capped at 16 (`extdepth`); a NULL child is `extcreate` + error |

`ext_encoding` was added for this work (commit "Harness: encodingName
argument ..."); `None` passes NULL as before, so all earlier tests and
corpora are unchanged. Both sides receive the same bytes, the same
`XML_ParserCreate(NS)` encoding and separator, `XML_SetParamEntityParsing`,
`XML_UseForeignDTD`, `XML_SetReturnNSTriplet`, the same not-standalone
handler, and the same hash salt (`ParseOptions` is one value passed to both
`expat_oracle::parse_trace` and `port_trace`). The children inherit the
parent's handlers, namespace setting, triplets, reparse deferral and
parameter entity parsing in both (C 1789-1826, port `external.rs`
329-338).

## 2. How a child's encoding is chosen (C, replicated by the port)

1. **`encodingName` argument, not the parent's encoding.** The child is
   created by `parserCreate(encodingName, ...)` (1778-1784), which copies
   the name into `m_protocolEncodingName` (`parserInit` 1482-1484). The
   parent's protocol encoding (`XML_ParserCreate(enc)` or
   `XML_SetEncoding`, 1654-1676, both set the same field) and the
   parent's detected or declared encoding are **not** inherited. The
   test confirms it: over 4 parent encodings (none/UTF-8, ISO-8859-1,
   UTF-16LE, UTF-16BE with BOM) the child's result never depends on the
   parent (only the parent's own positions change).
   The harness passes the parent encoding through `XML_ParserCreate`;
   `XML_SetEncoding` writes the same field before parsing and is not
   exercised separately.
2. **Unknown name.** On the child's first parse call,
   `externalEntityInitProcessor` / `externalParEntInitProcessor` (3458,
   5277) call `initializeEncoding` (5095-5122):
   `XmlInitEncoding(NS)` accepts `ISO-8859-1`, `US-ASCII`, `UTF-8`,
   `UTF-16`, `UTF-16BE`, `UTF-16LE` case-insensitively (`getEncodingIndex`,
   `xmltok.c` 1428) and NULL; any other name, including `""`, goes to
   `handleUnknownEncoding` (5219). Without an unknown-encoding handler
   this is `XML_ERROR_UNKNOWN_ENCODING` (18) before any token: the child
   reports line 1, column 0, byte index -1, and the handler (mode `Read`)
   returns the failure, so the parent stops with 21.
   `XML_ExternalEntityParserCreate` itself succeeds for an unknown name.
3. **Initial scan (`initScan`, `xmltok.c` 1456-1565).** With a known or
   NULL name, the first bytes decide: a general entity is scanned in the
   content state, the subset and parameter entities in the prolog state.
   - BOM `FE FF` / `FF FE`: UTF-16BE / UTF-16LE, **even if the name says
     UTF-8, US-ASCII or the other UTF-16 byte order**; only an
     `ISO-8859-1` name in the content state treats the bytes as data.
   - BOM `EF BB BF`: UTF-8, except in the content state with an
     `ISO-8859-1` or any `UTF-16*` name (then data).
   - `3C 00` (`<` in UTF-16LE): UTF-16LE, except in the content state
     with a `UTF-16` or `UTF-16BE` name; `00 xx`: UTF-16BE, except in the
     content state with a `UTF-16LE` name; `xx 00` (character data first):
     UTF-16LE only in the prolog state — a BOM-less UTF-16LE general
     entity starting with character data is read in the named / default
     encoding (C's comment at `xmltok.c` 1546-1554 explains the choice).
   - Otherwise the named encoding, or UTF-8 when the name is NULL.
   The port implements this in `tok::init::init_scan`
   (`crates/expat/src/tok/init.rs` 210-282).
4. **Text declaration.** For a general entity (`externalEntityInitProcessor3`
   3528) and for the subset / parameter entity between declarations
   (`doProlog`, `XML_ROLE_TEXT_DECL`, 5629-5630) the declaration is parsed
   as a text declaration (`processXmlDecl(parser, 1, ...)`): `encoding` is
   required, `version` optional, `standalone` forbidden; otherwise
   `XML_ERROR_TEXT_DECL` (31). A parameter entity read **inside an entity
   value** goes through `entityValueInitProcessor`, which calls
   `processXmlDecl(parser, 0, ...)` (5332): the XML declaration rules
   apply, so `<?xml encoding="UTF-8"?>` is `XML_ERROR_XML_DECL` (30) and
   `<?xml version="1.0"?>` is accepted. The port does the same
   (`parse/external.rs` 579 with `true`, 717 with `false`;
   `parse/prolog.rs` 247).
5. **Declared encoding.** Only when `m_protocolEncodingName` is NULL
   (5181): a known declared encoding must have the same minimum character
   size as the detected one and, for UTF-16, the same byte order,
   otherwise `XML_ERROR_INCORRECT_ENCODING` (19, at the name); if it
   matches it replaces the encoding (e.g. `ISO-8859-1` after a UTF-8 BOM).
   `UTF-16` matches either UTF-16 byte order (`findEncoding`,
   `xmltok_ns.c` 106). An unrecognised declared name is
   `handleUnknownEncoding` → 18 at the name. **When an `encodingName` was
   given, the declaration's encoding is ignored entirely**, even an
   unknown or inconsistent one (port: `process_xml_decl_encoding`,
   `parse/prolog.rs` 385-395). A misplaced declaration (not at the very
   start, e.g. after a newline) is `XML_ERROR_MISPLACED_XML_PI` (17).
6. **Unknown-encoding handler in the child.** C copies the parent's
   `m_unknownEncodingHandler` and its data into the child (1747-1748,
   1804-1805), so a child would consult it for an unknown name. Neither the
   oracle nor the port sets an unknown-encoding handler: the port has no
   unknown-encoding handler API yet (module 12, `STATE.md`). Only the
   "no handler" branch is compared here (error 18); the handler branch is
   **not verified** (gap G-1 below).

## 3. Errors and positions

- The child's error code, line, column and byte index are its own,
  relative to the entity (e.g. `u8-error-late`: `14@2:3`, byte 13). They
  are visible to the handler only (`extend` line); the parent reports
  21 with its position at the reference (`&e;` at byte 42 in the G
  document; the subset at the end of the DOCTYPE, byte 24; the `%p;`
  reference, byte 39).
- `XML_ERROR_UNKNOWN_ENCODING` from the name argument has no token
  position: byte index -1, column 0.
- A child that fails to be created (tracker refusal) is `extcreate`;
  not reached by these inputs.
- Nested children: a grandchild's failure makes the child's handler fail,
  so the child reports 21 and the parent 21 (`nested_children`
  "error-in-grandchild").

## 4. Matrix

Bodies (`ent`): the payload after the declaration is `<t>éx</t>` (G),
`<!ENTITY x "éx">` (S, P) or `éx` (V); `é` shows which encoding decoded
it. Prefixes: `u8` UTF-8, `lat` ISO-8859-1 bytes, `le` / `be` UTF-16LE /
BE; `-bom` with a byte order mark; `-decl8` / `-decl16` / `-decl16le` /
`-decl16be` / `-decllat` / `-declascii` / `-declunknown` / `-declempty`
a text declaration with that encoding (`x-unknown-enc`, `""`);
`-veronly` `<?xml version="1.0"?>`; `-ver8` version and encoding;
`-standalone` with `standalone="yes"`; `u8-bom-then-le` UTF-8 BOM followed
by UTF-16LE; `le-bom-odd` an odd trailing byte; `-textfirst` character data
first; `-error-late` a parse error on line 2; `empty`, `*-bom-only`.

Cells: the child's result as C reports it and the port reproduces it,
`ok` or `<error code>@<line>:<column>`; parent protocol encoding NULL,
whole-document feeding, parameter entity parsing ALWAYS, mode `Read`.
Codes: 4 invalid token, 5 unclosed token, 6 partial character, 7 tag
mismatch, 13 asynchronous entity, 14 bad character reference, 17
misplaced XML PI, 18 unknown encoding, 19 incorrect encoding, 21 external
entity handling, 30 XML declaration, 31 text declaration. With an error in
the child, the parent's result is 21 at the reference; with `ok` the
parent completes (in V, `éx` is used through `&x;`). `ok` means the
parse succeeded with C's events — not necessarily that `é` was decoded as
`é` (e.g. `le-nobom` with `UTF-16BE` reads U+3C00... as character data);
the full traces are compared.

Every one of these cells was also run with the three other parent
encodings and fed in 1-byte chunks and in 7-byte chunks through the
buffer API; Rust and C results per case (status, code, position, every
child's `extend`, handler return values) are in
`docs/evidence/ext-entity-encoding/matrix.tsv` (parent NULL rows and the
configuration rows; regenerate all with
`EXT_ENC_MATRIX=<file> cargo test -p expat-difftest --test ext_encoding_diff matrix_dump`).

#### G — general entity in content (context string, content state)

| body | NULL | `UTF-8` | `utf-8` | `ISO-8859-1` | `US-ASCII` | `UTF-16` | `UTF-16LE` | `UTF-16BE` | `x-unknown-enc` | `""` |
|---|---|---|---|---|---|---|---|---|---|---|
| `u8` | ok | ok | ok | ok | 4@1:3 | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom` | ok | ok | ok | ok | ok | 5@1:6 | 5@1:6 | 5@1:6 | 18@1:0 | 18@1:0 |
| `u8-decl8` | ok | ok | ok | ok | 4@1:27 | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-decl8-lower` | ok | ok | ok | ok | 4@1:27 | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom-decl8` | ok | ok | ok | 17@1:3 | ok | 5@1:18 | 5@1:18 | 5@1:18 | 18@1:0 | 18@1:0 |
| `u8-ver8` | ok | ok | ok | ok | 4@1:41 | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-veronly` | 31@1:19 | 31@1:19 | 31@1:19 | 31@1:19 | 31@1:19 | 5@1:15 | 5@1:15 | 5@1:15 | 18@1:0 | 18@1:0 |
| `u8-standalone` | 31@1:23 | 31@1:23 | 31@1:23 | 31@1:23 | 31@1:23 | 5@1:25 | 5@1:25 | 5@1:25 | 18@1:0 | 18@1:0 |
| `u8-decl16` | 19@1:16 | ok | ok | ok | 4@1:28 | 5@1:17 | 5@1:17 | 5@1:17 | 18@1:0 | 18@1:0 |
| `u8-decl16le` | 19@1:16 | ok | ok | ok | 4@1:30 | 5@1:18 | 5@1:18 | 5@1:18 | 18@1:0 | 18@1:0 |
| `u8-bom-decl16le` | 19@1:17 | ok | ok | 17@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom-decllat` | ok | ok | ok | 17@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-declascii` | 4@1:30 | ok | ok | ok | 4@1:30 | 5@1:18 | 5@1:18 | 5@1:18 | 18@1:0 | 18@1:0 |
| `u8-declunknown` | 18@1:16 | ok | ok | ok | 4@1:35 | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-declempty` | 31@1:16 | 31@1:16 | 31@1:16 | 31@1:16 | 31@1:16 | 5@1:14 | 5@1:14 | 5@1:14 | 18@1:0 | 18@1:0 |
| `lat-decllat` | ok | 4@1:32 | 4@1:32 | ok | 4@1:32 | ok | ok | ok | 18@1:0 | 18@1:0 |
| `lat-nodecl` | 4@1:3 | 4@1:3 | 4@1:3 | ok | 4@1:3 | 5@1:4 | 5@1:4 | 5@1:4 | 18@1:0 | 18@1:0 |
| `le-bom` | ok | ok | ok | 4@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-bom` | ok | ok | ok | 4@1:2 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-nobom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-nobom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl16` | ok | ok | ok | 4@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl16le` | ok | ok | ok | 4@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-bom-decl16be` | ok | ok | ok | 4@1:2 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-nobom-decl16le` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-nobom-decl16` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl16be` | 19@1:17 | ok | ok | 4@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-bom-decl16le` | 19@1:17 | ok | ok | 4@1:2 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl8` | 19@1:17 | ok | ok | 4@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decllat` | 19@1:17 | ok | ok | 4@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-veronly` | 31@1:20 | 31@1:20 | 31@1:20 | 4@1:3 | 31@1:20 | 31@1:20 | 31@1:20 | 31@1:20 | 18@1:0 | 18@1:0 |
| `le-bom-declunknown` | 18@1:17 | ok | ok | 4@1:3 | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `empty` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom-only` | ok | ok | ok | ok | ok | 5@1:1 | 5@1:1 | 5@1:1 | 18@1:0 | 18@1:0 |
| `le-bom-only` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-odd` | 5@1:10 | 5@1:10 | 5@1:10 | 4@1:3 | 5@1:10 | 5@1:10 | 5@1:10 | 5@1:10 | 18@1:0 | 18@1:0 |
| `u8-misplaced-decl` | 17@2:0 | 17@2:0 | 17@2:0 | 17@2:0 | 17@2:0 | 5@1:17 | 5@1:17 | 5@1:17 | 18@1:0 | 18@1:0 |
| `le-nobom-textfirst` | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:1 | 4@1:0 | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-nobom-textfirst` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom-then-le` | 4@1:2 | 4@1:2 | 4@1:2 | 4@1:4 | 4@1:2 | 13@1:6 | 5@1:10 | 13@1:6 | 18@1:0 | 18@1:0 |
| `u8-error-late` | 14@2:3 | 14@2:3 | 14@2:3 | 14@2:3 | 14@2:3 | 5@1:10 | 5@1:10 | 5@1:10 | 18@1:0 | 18@1:0 |
| `le-bom-error-late` | 7@2:5 | 7@2:5 | 7@2:5 | 4@1:3 | 7@2:5 | 7@2:5 | 7@2:5 | 7@2:5 | 18@1:0 | 18@1:0 |

#### S and P — external subset / parameter entity between declarations (NULL context, prolog state; S and P identical in every cell)

| body | NULL | `UTF-8` | `utf-8` | `ISO-8859-1` | `US-ASCII` | `UTF-16` | `UTF-16LE` | `UTF-16BE` | `x-unknown-enc` | `""` |
|---|---|---|---|---|---|---|---|---|---|---|
| `u8` | ok | ok | ok | ok | 4@1:12 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-bom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-decl8` | ok | ok | ok | ok | 4@1:36 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-decl8-lower` | ok | ok | ok | ok | 4@1:36 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-bom-decl8` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-ver8` | ok | ok | ok | ok | 4@1:50 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-veronly` | 31@1:19 | 31@1:19 | 31@1:19 | 31@1:19 | 31@1:19 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-standalone` | 31@1:23 | 31@1:23 | 31@1:23 | 31@1:23 | 31@1:23 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-decl16` | 19@1:16 | ok | ok | ok | 4@1:37 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-decl16le` | 19@1:16 | ok | ok | ok | 4@1:39 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-bom-decl16le` | 19@1:17 | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom-decllat` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-declascii` | 4@1:39 | ok | ok | ok | 4@1:39 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-declunknown` | 18@1:16 | ok | ok | ok | 4@1:44 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-declempty` | 31@1:16 | 31@1:16 | 31@1:16 | 31@1:16 | 31@1:16 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `lat-decllat` | ok | 4@1:41 | 4@1:41 | ok | 4@1:41 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `lat-nodecl` | 4@1:12 | 4@1:12 | 4@1:12 | ok | 4@1:12 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `le-bom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-bom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-nobom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-nobom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl16` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl16le` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-bom-decl16be` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-nobom-decl16le` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-nobom-decl16` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl16be` | 19@1:17 | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-bom-decl16le` | 19@1:17 | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl8` | 19@1:17 | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decllat` | 19@1:17 | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-veronly` | 31@1:20 | 31@1:20 | 31@1:20 | 31@1:20 | 31@1:20 | 31@1:20 | 31@1:20 | 31@1:20 | 18@1:0 | 18@1:0 |
| `le-bom-declunknown` | 18@1:17 | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `empty` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom-only` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-only` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-odd` | 5@1:17 | 5@1:17 | 5@1:17 | 5@1:17 | 5@1:17 | 5@1:17 | 5@1:17 | 5@1:17 | 18@1:0 | 18@1:0 |
| `u8-misplaced-decl` | 17@2:0 | 17@2:0 | 17@2:0 | 17@2:0 | 17@2:0 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `le-nobom-textfirst` | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `be-nobom-textfirst` | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-bom-then-le` | 4@1:2 | 4@1:2 | 4@1:2 | 4@1:2 | 4@1:2 | 4@1:2 | 4@1:2 | 4@1:2 | 18@1:0 | 18@1:0 |
| `u8-error-late` | 14@2:12 | 14@2:12 | 14@2:12 | 14@2:12 | 4@1:12 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `le-bom-error-late` | 2@2:11 | 2@2:11 | 2@2:11 | 2@2:11 | 2@2:11 | 2@2:11 | 2@2:11 | 2@2:11 | 18@1:0 | 18@1:0 |

#### V — parameter entity inside an entity value of the external subset (first child = the entity `ent`)

| body | NULL | `UTF-8` | `utf-8` | `ISO-8859-1` | `US-ASCII` | `UTF-16` | `UTF-16LE` | `UTF-16BE` | `x-unknown-enc` | `""` |
|---|---|---|---|---|---|---|---|---|---|---|
| `u8` | ok | ok | ok | 4@1:0 | 4@1:0 | 4@1:1 | 4@1:0 | 4@1:1 | 18@1:0 | 18@1:0 |
| `u8-bom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-decl8` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-decl8-lower` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-bom-decl8` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `u8-ver8` | ok | ok | ok | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-veronly` | ok | ok | ok | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-standalone` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-decl16` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-decl16le` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-bom-decl16le` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `u8-bom-decllat` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `u8-declascii` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-declunknown` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `u8-declempty` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `lat-decllat` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `lat-nodecl` | 6@1:0 | 6@1:0 | 6@1:0 | ok | 4@1:0 | 4@1:0 | ok | 4@1:0 | 18@1:0 | 18@1:0 |
| `le-bom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-bom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-nobom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-nobom` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-decl16` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `le-bom-decl16le` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `be-bom-decl16be` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `le-nobom-decl16le` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 18@1:0 | 18@1:0 |
| `be-nobom-decl16` | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 30@1:6 | 18@1:0 | 18@1:0 |
| `le-bom-decl16be` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `be-bom-decl16le` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `le-bom-decl8` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `le-bom-decllat` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `le-bom-veronly` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-declunknown` | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 30@1:7 | 18@1:0 | 18@1:0 |
| `empty` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom-only` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-only` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `le-bom-odd` | 4@1:3 | 4@1:3 | 4@1:3 | 4@1:3 | 4@1:3 | 4@1:3 | 4@1:3 | 4@1:3 | 18@1:0 | 18@1:0 |
| `u8-misplaced-decl` | 30@2:6 | 30@2:6 | 30@2:6 | 30@2:6 | 30@2:6 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `le-nobom-textfirst` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `be-nobom-textfirst` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |
| `u8-bom-then-le` | 4@1:1 | 4@1:1 | 4@1:1 | 4@1:1 | 4@1:1 | 4@1:1 | 4@1:1 | 4@1:1 | 18@1:0 | 18@1:0 |
| `u8-error-late` | 4@2:0 | 4@2:0 | 4@2:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 4@1:0 | 18@1:0 | 18@1:0 |
| `le-bom-error-late` | ok | ok | ok | ok | ok | ok | ok | ok | 18@1:0 | 18@1:0 |

### Configuration (body `u8`, `encodingName` NULL, whole document)

C result as recorded in the `CONFIG` rows of the evidence file (port
identical in all 288 rows; `configuration_matrix` additionally varies the
body, `encodingName`, namespaces / triplets, foreign DTD and feeding,
138,252 pairs):

| Context | Mode | Parameter entity parsing | `standalone="yes"`? | C result |
|---|---|---|---|---|
| G | None | any | any | ok, no handler call |
| G | Fail | any | any | 21 at the reference |
| G | Accept / Read | any | any | ok (Read: child `ok`) |
| S, P | any | NEVER | no | ok, no `extref`, `x` skipped; the not-standalone handler is called (if one is set) |
| S, P | None | UNLESS_STANDALONE / ALWAYS | no | ok, `x` skipped, no handler call |
| S, P | Fail | UNLESS_STANDALONE / ALWAYS | no | 21 |
| S, P | Accept | UNLESS_STANDALONE / ALWAYS | no | ok, `x` skipped (nothing read, so no not-standalone call) |
| S, P | Read | UNLESS_STANDALONE / ALWAYS | no | ok, child `ok`; not-standalone handler called after the read |
| S, P | any | NEVER / UNLESS_STANDALONE | yes | 11 (`x` undefined), no call |
| S, P | Fail | ALWAYS | yes | 21 |
| S, P | Accept | ALWAYS | yes | 11 |
| S, P | Read | ALWAYS | yes | 24 (`x` declared in an external entity, used in a standalone document) |

A not-standalone handler returning 0 turns the "handler called" rows into
`XML_ERROR_NOT_STANDALONE` (22); compared in `configuration_matrix`.

## 5. Verification

`tests/tests/ext_encoding_diff.rs` (runs in `cargo test --workspace`):

| Test | What | Pairs (C vs port) | Differences |
|---|---|---|---|
| `encoding_matrix` | 4 contexts (G, S, P, V) x 42 bodies x 10 `encodingName` values x 4 parent encodings x 3 feedings (whole; 1-byte chunks; 7-byte chunks via `XML_GetBuffer`/`XML_ParseBuffer`) | 20,160 | 0 |
| `configuration_matrix` | 4 contexts x `standalone` x 10 bodies x 4 `encodingName` x 4 handler modes x 3 parameter entity parsing modes x 3 namespace settings (none, `|`, `|` + triplets) x 3 not-standalone handlers x foreign DTD x 2 feedings, plus a foreign-DTD-only document | 138,252 | 0 |
| `nested_children` | the name applies to grandchildren; G in G (with / without BOM), P in S, error in a grandchild; 10 names x 3 feedings | 120 | 0 |
| `c_behaviour_pins` | 24 C results stated in this document, pinned (code and the child's `extend` line) | 24 | 0 |
| `matrix_dump` | writes the per-case table when `EXT_ENC_MATRIX` is set | — | — |

Per case the test records C and the port separately (`Outcome`: status,
code, line, column, byte index, the `extend` line of every child, the
handler's return value for every `extref`, the full trace) and, on a
difference, prints both records and the first differing trace line.

Sensitivity (mutations applied one at a time to a scratch copy, never
committed; logs outside the repository):

| Mutation | Where | Caught by |
|---|---|---|
| M1 UTF-16BE BOM honoured with an ISO-8859-1 name in content | `tok/init.rs` | 24 pairs |
| M2 UTF-8 BOM honoured in content with a `UTF-16` name | `tok/init.rs` | 72 pairs |
| M3 declared encoding applied although an `encodingName` was given | `parse/prolog.rs` | 2,496 + 12 nested + 2 pins |
| M4 `encodingName` ignored by `external_entity_parser_create` | `parse/external.rs` | 9,552 + 12,204 + 81 + 7 pins |
| M5 `00 xx` always UTF-16BE (UTF-16LE name in content ignored) | `tok/init.rs` | 36 + 216 |
| M6 text declaration rules for a PE inside an entity value | `parse/external.rs` | 2,004 + 1,512 + 2 pins |
| harness: the port adapter passes NULL instead of `ext_encoding` | `tests/src/port.rs` | 7,860 + 10,152 + 81 (before V was added) |

## 6. Gaps and assumptions (not verified here)

- **G-1** Unknown-encoding handler (`XML_SetUnknownEncodingHandler`) in the
  parent or child: not compared, the port has no such handler yet
  (module 12). C would call the inherited handler for an unknown
  `encodingName` or declared name.
- **G-2** `XML_SetEncoding` on the parent is not exercised separately; it
  writes the same `m_protocolEncodingName` as `XML_ParserCreate(enc)`
  (source reading, 1654-1676), and the child ignores it either way.
- **G-3** The `extcreate` path (child creation refused by the allocation
  tracker) and `XML_StopParser` / suspension inside a child are not part
  of this matrix (tracker refusals are covered by `tracker_diff` /
  `external_parser_diff`; suspension is module 12).
- **G-4** `XML_UNICODE` builds are out of scope (D-002); the
  `XML_UNICODE` branch of `initializeEncoding` (5097-5112) is not compiled.
- Platform: run locally on aarch64 Linux (Debian, gcc 14.2 for the
  oracle); in CI the test runs on every platform through
  `cargo test --workspace`.
- Encoding names with an interior NUL cannot be passed to C (the harness
  rejects them before the call); not tested.
