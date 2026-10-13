# DTD, entities and their security limits in libexpat 2.9.0

Written before the port of module 11 (DTD, entities, amplification limits)
as the specification the port is tested against. Every statement cites the
vendored, unmodified source: `crates/expat-oracle/vendor/expat/lib/xmlparse.c`
(`R_2_9_0`, `a76b1174`), `internal.h` and `expat.h` in the same directory.
Line numbers refer to those files. The build configuration is the one of
`crates/expat-oracle/shim/expat_config.h` (D-002): `XML_DTD`, `XML_GE == 1`,
`XML_NS`, `XML_CONTEXT_BYTES 1024`.

## 1. What C supports

| Feature | Where | Behaviour |
|---|---|---|
| Document type declaration, internal subset | `doProlog` 5484-6607, roles from `xmlrole.c` | Reported through the doctype, element, attlist, entity, notation handlers. |
| Internal general entities | declared at `XML_ROLE_GENERAL_ENTITY_NAME` 6093-6125 and `XML_ROLE_ENTITY_VALUE` 5929-5982 (`callStoreEntityValue` 7300-7364, `storeEntityValue` 7111-7297) | Replacement text is stored as UTF-8. The first declaration wins (`lookup` returns the existing entry, the new name is discarded, 6103-6110). Predefined names (`lt`, `gt`, `amp`, `quot`, `apos`) are never declared (6094-6097). |
| Expansion in content | `doContent` 3660-3745, `processEntity` 6688-6744, `internalEntityProcessor` 6747-6823 | Non-recursive: open entities form a stack (`m_openInternalEntities`), each step returns to `callProcessor`'s loop through `m_reenter` (1211-1273). |
| Expansion in attribute values | `appendAttributeValue` 6916-7108, `storeAttributeValue` 6838-6914 | Separate stack `m_openAttributeEntities`; external and unparsed entities are errors there (7069-7078). |
| Default attribute values | `XML_ROLE_DEFAULT_ATTRIBUTE_VALUE` 5889-5928, `defineAttribute` 7526-7590, applied by `storeAtts` 4291-4314 | The value is normalised at declaration time with `XML_ACCOUNT_NONE`; the first definition of an attribute is binding. |
| Internal parameter entities | declared at `XML_ROLE_PARAM_ENTITY_NAME` 6126-6158; referenced at `XML_ROLE_PARAM_ENTITY_REF`/`INNER_PARAM_ENTITY_REF` 6314-6420 | Expanded only when `m_paramEntityParsing != NEVER` (6317-6318); otherwise `keepProcessing = standalone` and later declarations are skipped. In entity values they are expanded only inside a parameter entity or external subset (`m_isParamEntity` or a non-document encoding, 7155); in the internal subset itself a PE reference in an entity value is `XML_ERROR_PARAM_ENTITY_REF` (7214-7218). |
| External general entities | `doContent` 3724-3743 | Only reported to `XML_SetExternalEntityRefHandler`'s handler (2169), which must create an external entity parser itself (`XML_ExternalEntityParserCreate` 1680). Without a handler the reference is passed to the default handler or dropped. |
| External parameter entities and the external subset | 5682-5753 (doctype close), 5754-5795 (foreign DTD), 6382-6405 (PE references), 7186-7206 (in entity values) | Only through the same handler and only when `m_paramEntityParsing != NEVER` (5709, 6384 path via 6314). |
| Unparsed entities and notations | 6065-6092, 6159-6215 | Reported, never read. A reference to an unparsed entity is `XML_ERROR_BINARY_ENTITY_REF` (3709-3710, 7069-7073). |
| Conditional sections | `doIgnoreSection` 5012-5092, `XML_ROLE_IGNORE_SECT` 6228-6242 | Only in external parameter entities / the external subset (the role machine rejects them in the internal subset). |

## 2. External resource access

- **libexpat never opens a file or a network connection.** The only way an
  external entity is read is the application's
  `XML_ExternalEntityRefHandler`, which receives base, system id and public
  id and decides what to do (2169-2181, 3724-3743, 5717-5727, 6386-6397).
  Without that handler, external entities are skipped (content: 3741-3743)
  or leave `keepProcessing = standalone` (prolog: 6405-6408).
- **Parameter entity parsing defaults to `XML_PARAM_ENTITY_PARSING_NEVER`**
  (`parserInit`, 1553). `XML_SetParamEntityParsing` (2226-2238) refuses
  changes once parsing has started (`parserBusy`).
  `XML_PARAM_ENTITY_PARSING_UNLESS_STANDALONE` turns into `NEVER` when the
  XML declaration says `standalone="yes"` (`processXmlDecl` 5150-5157).
- **The external subset** (`<!DOCTYPE d SYSTEM "x">`) is requested only if
  parameter entity parsing is on *and* a handler is set (5705-5709);
  `XML_UseForeignDTD` (1951) asks for one even without a doctype (5755).
- **Standalone.** `XML_SetNotStandaloneHandler` (2162) is called when a
  non-standalone document refers to external markup (5998-6013, 5728-5736,
  6410-6418); returning 0 gives `XML_ERROR_NOT_STANDALONE`.
- **Entities declared in parameter entities.** When the document is
  standalone or has no PE references, every referenced entity must be
  declared (`XML_ERROR_UNDEFINED_ENTITY`, 3692-3697, 6333-6359, 7015-7029)
  and declared outside a PE (`is_internal`, else
  `XML_ERROR_ENTITY_DECLARED_IN_PE`). Otherwise an undeclared reference is
  reported to the skipped-entity handler (3698-3705, 6362-6370) and is not
  an error.

- **External entity parsers (module 11d step D2).** What the handler
  feeds into `XML_ExternalEntityParserCreate`'s child is parsed with the
  same checks as the document: text declaration rules, conditional
  sections only in external PEs / the subset, recursion through the open
  entities passed in the context string (`XML_ERROR_RECURSIVE_ENTITY_REF`),
  proper nesting. Every child counts its bytes as *indirect* in the root's
  billion-laughs accounting and allocates through the root's tracker.
- **Security note: libexpat does not limit the number or depth of
  external entity parsers.** Measured (`external_parser_diff.rs`, default
  limits): a document whose external entities each reference the next
  one four times, nine levels deep, makes C create and run **87,381
  nested external entity parsers** (general or parameter entities, 0.3-0.7
  s) without any error, because the total bytes stay below the 8 MiB
  activation threshold of the amplification check and the tracker's
  64 MiB threshold. Only the application's handler can stop this (the
  test handler caps nesting at 16). The port behaves identically
  (D-026: defaults are C's); an application that reads external entities
  must cap their depth and count in its handler, or lower the
  billion-laughs activation threshold (with a 4 KiB threshold the same
  documents stop with 43 after 120-692 children).

## 3. Recursion

- Every expansion sets `entity->open` (6723) until the entity and
  all entities opened inside it are closed (6806-6822, 6874-6893,
  7343-7354). A reference to an open entity is
  `XML_ERROR_RECURSIVE_ENTITY_REF`: content 3707-3708, parameter entities
  6373-6374, attribute values 7046-7068, entity values 7178-7183 (which
  also rejects a reference to the entity being declared,
  `entity == m_declEntity`). Direct (`<!ENTITY a "&a;">`) and indirect
  (`a -> b -> a`) recursion are detected the same way, at the reference
  that closes the cycle, when it is expanded (declaration alone is not an
  error for general entities, whose values are not expanded when stored).
- **Position of the error.** `m_eventPtr` is only moved inside the document
  entity; while an entity's text is processed the event pointers of the
  open entity are used (`internalEventPtr`, 3587-3593, 5524-5530; `m_eventPtr` is set at the top of each `doProlog` iteration, 5535), so the
  reported line/column/byte index of an error inside an expansion is the
  position of the outermost reference in the document (where
  `m_eventPtr` was last set).
- Expansion is iterative since 2.7.0 (`callProcessor` loop 1239-1260,
  `triggerReenter` in `processEntity` 6739-6742 and `internalEntityProcessor` 6801, 6824): nesting depth does not
  consume C stack. Content models are built iteratively too (`build_model`
  8748-8884, comment at 8790-8800).
- An entity must be properly nested: an element started in an entity must
  end in it (`XML_ERROR_ASYNC_ENTITY`, 3629-3641, 3864-3866, 6796-6800).

## 4. Billion laughs protection (amplification accounting)

- Defaults (`internal.h` 104-111, applied in `parserInit` 1560-1566):
  **maximum amplification 100.0**, **activation threshold 8,388,608 bytes
  (8 MiB, 2^23)**.
- Accounting (`accountingDiffTolerated` 9016-9071,
  `accountingGetCurrentAmplification` 8937-8951): each token's bytes are
  added to *direct* (document entity, root parser) or *indirect* (entity
  expansion and external entity parsers) counters. A token is tolerated
  while `direct + indirect < threshold` or
  `(direct + indirect) / direct <= maximum` (computed in `float`; with no
  direct bytes the denominator is 22, the length of
  `<!ENTITY a SYSTEM 'b'>`). Otherwise the parser stops with
  `XML_ERROR_AMPLIFICATION_LIMIT_BREACH` (43). Counter overflow also
  stops it (9046-9048).
- Predefined entities and character references are counted without being
  checked (3666-3677, 6993-7000). Default attribute values are stored with
  `XML_ACCOUNT_NONE` (5896); their bytes count when the attribute list is
  read.
- `XML_SetBillionLaughsAttackProtectionMaximumAmplification` (3099-3124):
  rejects NaN and values < 1.0, and non-root parsers.
  `XML_SetBillionLaughsAttackProtectionActivationThreshold` (3126-3148):
  any value, root parser only. Both may be called at any time.
- There is no separate limit on nesting depth, number of entities or
  replacement text length; quadratic blow-up (one large entity referenced
  many times) is bounded only by the same amplification check.
- Measured with the differential tests (module 11b2,
  `tests/tests/amplification_diff.rs`): ten levels of ten references
  (3 * 10^9 bytes unchecked) stop with 43 just after the output passes
  8 MiB in content, attribute values, default values, parameter entities
  and entity values; six levels (3 MB) are accepted (below the
  activation threshold); 195 KB of direct text followed by 9.2 MB of
  expansion (factor ~48) is accepted; a 16 KiB entity referenced 1,024
  times (16 MiB from 20 KB) stops with 43 by default and is accepted with
  a factor of 1,000. The port gives the same result, position and byte
  counters in every case.
- Known C defect in the accounting, without effect on results: Q-4 in
  DIVERGENCES.md (`epilogProcessor` adds `NULL - s` to the direct counter
  for a name at the end of the buffer, then fails with error 9).

### Bounds of the resource-limit tests

Every comparison in the amplification tests runs through
`expat_difftest::bounded::compare`, which enforces three bounds per pair so
that a wrong limit on either side cannot hang or exhaust the machine:

| Bound | Small-document groups | Large-expansion groups | Mechanism |
|---|---|---|---|
| Input size | 16 KiB | 256 KiB | asserted before parsing; a larger document is a test bug |
| Wall clock (C + port) | 20 s | 60 s | parse on a worker thread, `recv_timeout`; the test fails on expiry |
| Port heap | 64 MiB | 256 MiB | per-thread counting `#[global_allocator]` (`expat_oracle::counting_alloc`); a request above the cap is refused, which aborts the test binary |
| C heap | 64 MiB | 256 MiB | oracle parser created with `XML_ParserCreate_MM` and a capped allocator (`ParseOptions::heap_cap`); a refusal is `XML_ERROR_NO_MEMORY` and shows as a mismatch |

Measured peaks (release build, 2026-10-12): quadratic blow-up in an
attribute value 39.9 MB port / 33.6 MB C, default-limit billion laughs
8.4 MB port / 4.2 MB C, small groups below 15 KB; slowest pair 0.42 s.
Large expansions are traced with `text_digest` (character data as length
and FNV-1a hash), so traces stay small.

## 5. Allocation tracker

### What C does

- Defaults (`internal.h` 113-115, `parserCreate` 1360-1370): maximum
  amplification **100.0** of bytes allocated per direct input byte,
  activation threshold **67,108,864 bytes (64 MiB, 2^26)**.
- Every `MALLOC`/`REALLOC` of the parser (`expat_malloc` 898-950,
  `expat_realloc` 987-1060, `expat_heap_increase_tolerable` 860-894)
  counts `sizeof(size_t) + EXPAT_MALLOC_PADDING + size` (8 + size on every
  supported target); `FREE` subtracts it. An allocation is refused when
  the new total is at or above the threshold **and** `total / direct`
  exceeds the maximum (computed in `float`; with nothing read directly the
  ratio is infinite). A refusal surfaces as `XML_ERROR_NO_MEMORY` (1) at
  the allocation site.
- **Not tracked:** the input buffer (`XML_GetBuffer`, allocated with the
  plain `malloc_fcn`: "We are avoiding MALLOC(..) here to leave limiting
  the input size to the application using Expat", 2601-2603) and the
  content models handed to the element declaration handler (6442, 8780).
  Character data expanded from entities in content is streamed through
  the fixed data buffer and does not grow the total either; attribute
  values and entity values are built in string pools and do.
- Setters: `XML_SetAllocTrackerMaximumAmplification` (3150-3176: NaN and
  values below 1.0 refused, non-root parsers refused) and
  `XML_SetAllocTrackerActivationThreshold` (3178-3200: any value, root
  parser only).
- Example only the tracker stops: 85,000 references to a 395-character
  entity in one attribute value (340,434 bytes of input, billion-laughs
  factor 99.75, below 100). The value's pool block doubles to 64 MiB
  (33,563,484 bytes tracked when the next doubling is refused), so C
  reports error 1 at the default limits; 84,000 references are accepted.

### What the port does (D-030, D-034)

- The port's tracker (`parse::tracker`) replays C's allocations event by
  event (D-034): the parser's creation and `XML_ParserFree` as C's
  individual allocations, the string pools (`PoolShadow`: `poolGrow`,
  `poolAppend` writing whole characters, `poolAppendChars`), hash tables,
  tags and their buffers (`storeRawNames`), bindings, attribute arrays,
  open-entity records, default-attribute arrays, the scaffold and every
  external entity parser, with every structure size computed from C's
  field list for the target (`parse::c_layout`) and compared with the C
  compiler's `sizeof` on every CI platform (`tests/tests/tracker_sizes.rs`).
- Like C, the port does **not** charge the input buffer, and it does not
  charge expanded character data in content.
- Setters with C's validation (`Parser::set_alloc_tracker_*`); defaults
  100.0 and 64 MiB, unchanged (D-026).
- Where C tolerates a refused allocation, so does the port (an `xmlns:`
  prefix whose `lookup` C does not check, `xmlparse.c` 7650-7652); where
  C reports partial state after a refusal, the port reports the same (the
  entity value handed to the declaration handler, 5935-5958).
- Relation: C and the port refuse exactly the same documents at the same
  allocation with the same running totals (`tracker_events.rs`,
  `tracker_stages.rs`, the fuzz targets' relation in
  `tests/src/tracker.rs`). This was violated in both directions before
  D-034: R1 (two sizes 8 bytes below C's: C refused ~687 KB documents
  the port accepted) and DV-1 (the port refused documents C accepted, W1
  and W2); `tests/tests/dv1_windows.rs` pins the boundaries, which are
  now equal. Per-platform results: `docs/REPORT.md`, "DV-1 parity,
  stages 2-8".

## 6. Error codes and positions relevant to the DTD

| Code | Value | Raised at |
|---|---|---|
| `XML_ERROR_SYNTAX` | 2 | role machine error (6216-6227), connector mismatch in models (6283, 6290) |
| `XML_ERROR_INVALID_TOKEN` | 4 | closing the doctype inside a PE (5694-5697), entity value tokens (7259-7268) |
| `XML_ERROR_PARAM_ENTITY_REF` | 10 | PE reference inside a declaration in the internal subset (6219-6221, 7214-7218) |
| `XML_ERROR_UNDEFINED_ENTITY` | 11 | 3695, 6337, 7027 |
| `XML_ERROR_RECURSIVE_ENTITY_REF` | 12 | 3708, 6374, 7067, 7182 |
| `XML_ERROR_ASYNC_ENTITY` | 13 | 3631, 3641, 3866, 4025, 6799 |
| `XML_ERROR_BAD_CHAR_REF` | 14 | entity values 7238-7245, attribute values 6943-6949 |
| `XML_ERROR_BINARY_ENTITY_REF` | 15 | 3710, 7072 |
| `XML_ERROR_ATTRIBUTE_EXTERNAL_ENTITY_REF` | 16 | 7077 |
| `XML_ERROR_EXTERNAL_ENTITY_HANDLING` | 21 | handler returned 0 (3739, 5726, 5776, 6396, 7198) |
| `XML_ERROR_NOT_STANDALONE` | 22 | 5734, 5784, 6012, 6416 |
| `XML_ERROR_ENTITY_DECLARED_IN_PE` | 24 | 3697, 6359, 7029 |
| `XML_ERROR_INCOMPLETE_PE` | 29 | a PE between declarations that ends inside markup (5561-5569) |
| `XML_ERROR_PUBLICID` | 32 | `XmlIsPublicId` failure, which moves `m_eventPtr` to the bad character (5648, 5663, 6172) |
| `XML_ERROR_AMPLIFICATION_LIMIT_BREACH` | 43 | every `accountingDiffTolerated` call site |

Positions: for errors in the document entity, `m_eventPtr` is the start of
the token being processed (set at the top of `doProlog`'s loop, 5535),
except where the code above moves it (invalid token: its end; public id:
the bad character; entity value/attribute value errors in the document
encoding: the offending reference, 7181, 7216, 7243, 7264, 7269). Inside
an expansion `m_eventPtr` is not moved (section 3).

## 7. What the port does (decision D-026)

- **Same defaults as C, no stricter defaults.** The port keeps
  `ParamEntityParsing::Never`, the amplification defaults (100.0, 8 MiB)
  and the same checks at the same places. A stricter default (for
  example disabling internal entity expansion, or a lower amplification
  limit) would change results for valid documents that C accepts, so it
  would be a divergence; none is added. The existing setters already let
  a caller tighten the limits (opt-in), exactly as in C.
- **No I/O in the library.** The core crate is `no_std` and has no file
  or network access; external entities can only be read by the caller's
  handler (`parse::Handler::external_entity_ref`, module 11d; refuses by
  default). Without a handler set the port behaves like C without
  `XML_SetExternalEntityRefHandler`.
- **No check is relaxed** to make a differential test pass; a mismatch is
  fixed at its root cause or recorded in DIVERGENCES.md with a
  specification reference.
