/* Build shim (not part of upstream): C side of the oracle FFI.
 * Mirrored by crates/expat-oracle/src/ffi.rs. */
#ifndef EXPAT_RS_ORACLE_H
#define EXPAT_RS_ORACLE_H

#include <stddef.h>

enum {
  ORACLE_TABLE_UTF8 = 0,
  ORACLE_TABLE_UTF8_NS = 1,
  ORACLE_TABLE_INTERNAL_UTF8 = 2,
  ORACLE_TABLE_INTERNAL_UTF8_NS = 3,
  ORACLE_TABLE_LATIN1 = 4,
  ORACLE_TABLE_LATIN1_NS = 5,
  ORACLE_TABLE_ASCII = 6,
  ORACLE_TABLE_ASCII_NS = 7,
  ORACLE_TABLE_LITTLE2 = 8,
  ORACLE_TABLE_LITTLE2_NS = 9,
  /* Encodings only (no separate table: same as LITTLE2). */
  ORACLE_TABLE_BIG2 = 10,
  ORACLE_TABLE_BIG2_NS = 11
};

enum {
  ORACLE_PRED_IS_NAME2 = 0,
  ORACLE_PRED_IS_NAME3 = 1,
  ORACLE_PRED_IS_NMSTRT2 = 2,
  ORACLE_PRED_IS_NMSTRT3 = 3,
  ORACLE_PRED_IS_INVALID2 = 4,
  ORACLE_PRED_IS_INVALID3 = 5,
  ORACLE_PRED_IS_INVALID4 = 6
};

/* Parse options for oracle_parse_trace. */
struct oracle_options {
  const char *encoding;  /* NULL: autodetect */
  int ns_separator;      /* 0: no namespace processing; else separator byte */
  size_t chunk_size;     /* 0: feed whole document in one XML_Parse call */
  int param_entity_parsing; /* XML_ParamEntityParsing value */
  int buffer_api; /* 1: XML_GetBuffer(2n+3) + XML_ParseBuffer(n) per chunk */
  int no_reparse_deferral; /* 1: XML_SetReparseDeferralEnabled(p, 0) */
  int ns_triplets; /* 1: XML_SetReturnNSTriplet(p, 1) */
  /* Module 11 resource limits. Each setter is called only if its set_*
     flag is 1; its XML_Bool result is written to the trace as a "limit"
     line before the document is parsed. */
  int set_bl_amplification; /* XML_SetBillionLaughsAttackProtectionMaximumAmplification */
  float bl_amplification;
  int set_bl_threshold; /* XML_SetBillionLaughsAttackProtectionActivationThreshold */
  unsigned long long bl_threshold;
  int set_at_amplification; /* XML_SetAllocTrackerMaximumAmplification */
  float at_amplification;
  int set_at_threshold; /* XML_SetAllocTrackerActivationThreshold */
  unsigned long long at_threshold;
  /* 1: character data is traced as "textdigest <len> <fnv1a64>" instead of
     the text itself, so traces of large expansions stay small. */
  int text_digest;
  /* 1: an "accounting <direct> <indirect>" line (billion-laughs counters of
     the root parser) precedes the result line. */
  int trace_accounting;
  /* != 0: the parser is created with XML_ParserCreate_MM whose allocator
     refuses any request that would take the parser's live heap above this
     many bytes (a memory cap for bounded tests; the refusal is an ordinary
     allocation failure inside libexpat). oracle_last_heap_peak() returns
     the peak live heap of the calling thread's last such parse. */
  size_t heap_cap;
  /* Module 11d: external entities through a test handler. The library
     never fetches anything; these handlers only trace their arguments.
     ext_handler: 0 no XML_SetExternalEntityRefHandler; 1 a handler that
     traces "extref <context> <base> <systemId> <publicId>" and returns
     XML_STATUS_ERROR; 2 the same handler returning XML_STATUS_OK without
     reading anything. not_standalone: 0 no XML_SetNotStandaloneHandler; 1
     a handler that traces "notstandalone" and returns 1; 2 the same,
     returning 0. use_foreign_dtd: 1 calls XML_UseForeignDTD(p, XML_TRUE).
     ext_handler 3 (step D2): after tracing, the handler looks the system
     identifier up in ext_data (records: u32 LE length + system id, u32 LE
     length + bytes) and, if found, reads the bytes from memory through
     XML_ExternalEntityParserCreate(p, context, ext_encoding), fed like the
     document (chunk_size, buffer_api), traces
     "extend <ok> <code> <line> <column> <byteindex>" for the child and
     returns its status; not found: XML_STATUS_OK without reading. Nesting
     is capped at ORACLE_EXT_MAX_DEPTH children ("extdepth", error);
     a NULL from XML_ExternalEntityParserCreate traces "extcreate" and
     fails. */
  int ext_handler;
  int not_standalone;
  int use_foreign_dtd;
  /* 1: XML_SetUnparsedEntityDeclHandler with a handler that traces
     "unparsed <name> <base> <systemId> <publicId> <notation>". */
  int unparsed_handler;
  const unsigned char *ext_data;
  size_t ext_len;
  /* The encodingName argument ext_handler 3 passes to every
     XML_ExternalEntityParserCreate call (NULL: no name, the default). */
  const char *ext_encoding;
};

#define ORACLE_EXT_MAX_DEPTH 16

int oracle_type_table(int id, unsigned char out[256]);
int oracle_ucs2_naming(int which, unsigned int hi, unsigned int lo);
int oracle_utf8_predicate(int pred, const unsigned char *p, size_t len);
int oracle_utf8_encode(int c, unsigned char out[4]);
int oracle_utf16_encode(int c, unsigned short out[2]);
/* SipHash-2-4 from siphash.h; data is fed to sip24_update in pieces of
 * `split` bytes (0 = one call). */
unsigned long long oracle_siphash24(const unsigned char key[16],
                                    const unsigned char *data, size_t len,
                                    size_t split);
int oracle_sip24_valid(void);

/* The built-in ENCODING with ORACLE_TABLE_* id `id` (NULL if unknown). */
const void *oracle_encoding(int id);

/* xmltok_impl.c through the vtable of `enc`, an ENCODING from
 * oracle_encoding() or oracle_unknown_enc(). `which`: 0 prolog, 1 content, 2 CDATA section,
 * 3 ignore section, 4 attribute value, 5 entity value. Offsets are relative
 * to `buf`; -1 means the C function did not write the pointer. Functions
 * without a `len` read until a terminator, like the C ones: only call them on
 * input that satisfies their precondition. */
int oracle_tok_scan(const void *enc, int which, const char *buf, size_t len,
                    size_t ptr, long long *next);
int oracle_tok_is_public_id(const void *enc, const char *buf, size_t len, size_t ptr,
                            long long *bad);
int oracle_tok_get_atts(const void *enc, const char *buf, size_t ptr, int atts_max,
                        long long *out);
int oracle_tok_char_ref_number(const void *enc, const char *buf, size_t ptr);
int oracle_tok_predefined_entity_name(const void *enc, const char *buf, size_t len,
                                      size_t ptr);
int oracle_tok_name_matches_ascii(const void *enc, const char *buf, size_t len,
                                  size_t ptr, const char *ascii);
long long oracle_tok_name_length(const void *enc, const char *buf, size_t ptr);
long long oracle_tok_skip_s(const void *enc, const char *buf, size_t ptr);
void oracle_tok_update_position(const void *enc, const char *buf, size_t len,
                                size_t ptr, unsigned long long pos[2]);

/* utf8Convert (to_utf16 == 0, `to` is char[to_len]) or utf16Convert
 * (`to` is unsigned short[to_len]) of `enc` on from[0..from_len]. Returns the
 * XML_Convert_Result; *read and *written are how far fromP and toP moved.
 * For an unknown encoding the input must end on a sequence boundary. */
int oracle_convert(const void *enc, int to_utf16, const char *from,
                   size_t from_len, void *to, size_t to_len, size_t *read,
                   size_t *written);
/* _INTERNAL_trim_to_complete_utf8_characters: the trimmed length. */
size_t oracle_trim_utf8(const char *from, size_t len);

/* XmlInitUnknownEncoding(NS) with `map`; `tab` (65536 ints) defines the
 * converter (see oracle_internals.c), NULL for none. Returns a handle, or
 * NULL if the C function rejects the map. */
void *oracle_unknown_new(const int map[256], const int *tab, int ns);
const void *oracle_unknown_enc(const void *h);
void oracle_unknown_free(void *h);

/* initScan: XmlInitEncoding(NS) with `name` (NULL: none), then XmlTok in
 * `state` (0 prolog, 1 content) on buf[ptr..len]. *selected is the builtin
 * id that *encPtr points to afterwards, or -1 if it still points to the
 * initial encoding. Returns ORACLE_INIT_FAILED if the name is rejected. */
enum {
  ORACLE_INIT_FAILED = -100,
  ORACLE_UNTOUCHED = -7,
  ORACLE_NULL = -2,
  ORACLE_SAME = -3
};
int oracle_init_scan(const char *name, int ns, int state, const char *buf,
                     size_t len, size_t ptr, long long *next, int *selected);
/* XmlUpdatePosition of a fresh INIT_ENCODING (initUpdatePosition). */
void oracle_init_update_position(const char *buf, size_t len, size_t ptr,
                                 unsigned long long pos[2]);
/* XmlParseXmlDecl(NS) on buf[ptr..end]. out: bad, version, version_end,
 * encoding name offsets (-1 = not written), then the found encoding:
 * ORACLE_UNTOUCHED, ORACLE_NULL, ORACLE_SAME (the input encoding, when it is
 * not a builtin) or a builtin id. *standalone is ORACLE_UNTOUCHED, 0 or 1.
 * The caller pads the buffer (unknown converters read past `end`). */
int oracle_parse_xml_decl(int ns, int is_general_text_entity, const void *enc,
                          const char *buf, size_t ptr, size_t end,
                          long long out[5], int *standalone);

/* xmlrole.c (shim/oracle_role.c): the prolog state after a token. */
struct oracle_role_state {
  int handler; /* index into the handler table, -1 if unknown */
  unsigned level;
  int role_none;
  unsigned include_level;
  int document_entity;
};
/* Runs the n tokens (toks[i], buf[ptrs[i]..ends[i]]) through a fresh
 * PROLOG_STATE (XmlPrologStateInit, then XmlPrologStateInitExternalEntity
 * if `external`, then overwritten by `init` unless it is NULL), writing the
 * role and the state after each token. Returns -1 for a bad handler index. */
int oracle_role_run(int external, const struct oracle_role_state *init,
                    const void *enc, const char *buf, size_t n, const int *toks,
                    const size_t *ptrs, const size_t *ends, int *roles,
                    struct oracle_role_state *states);

/* Runs the full parser and writes a canonical event trace (see
 * docs/ARCHITECTURE.md, "Trace format") into a malloc'd buffer.
 * Returns 0 on success (the parse itself may have failed; that is part of
 * the trace), -1 on allocation failure in the shim. */
int oracle_parse_trace(const unsigned char *doc, size_t len,
                       const struct oracle_options *opts, unsigned char **out,
                       size_t *out_len);
void oracle_free(void *p);
size_t oracle_last_heap_peak(void);
/* Allocation tracker byte count (m_alloc_tracker.bytesAllocated) of the
   calling thread's last oracle_parse_trace: after creation and the limit
   setters, and at the end of the parse. */
void oracle_last_tracker(unsigned long long *created, unsigned long long *end);
/* sizeof / offsetof of the tracked C structures (D-034), in the order of
   ORACLE_C_SIZE_NAMES in src/lib.rs; returns the number written. */
size_t oracle_c_sizes(size_t *out, size_t n);

/* Allocation tracker events of the full parser (shim/oracle_xmlparse.c,
   D-034): every granted malloc / realloc / free of the tracker, and every
   refusal, of parsers created on the calling thread while a capture is on.
   `size` is the reported change (header included for malloc and free, the
   size difference for realloc), `total` the live count after the event
   (unchanged by a refusal), `peak` peakBytesAllocated, `direct` the root's
   countBytesDirect, `line` the xmlparse.c line, `op` '+' or '-'. */
struct oracle_tracker_event {
  unsigned long long size;
  unsigned long long total;
  unsigned long long peak;
  unsigned long long direct;
  int line;
  char op;
  char refused;
};
/* Starts (on != 0) or stops a capture on the calling thread; discards
   the events kept so far. Must be on before XML_ParserCreate. */
void oracle_tracker_events_capture(int on);
/* Records the number of events so far as the end of the parse (called by
   oracle_parse_trace just before XML_ParserFree). */
void oracle_tracker_events_mark_end(void);
/* Hands over the events (free with oracle_free) and stops the capture;
   *complete is 0 when events were dropped (memory); *at_end is the number
   of events before XML_ParserFree. */
size_t oracle_tracker_events_take(struct oracle_tracker_event **out,
                                  int *complete, size_t *at_end);

/* xmlparse.c string pool and hash table (shim/oracle_core.c). Runs a
 * program of pool or hash table operations (format: tests/src/core.rs) and
 * writes the text trace into a malloc'd buffer. Not thread-safe: callers
 * serialise runs. Returns -1 for a malformed header or a shim allocation
 * failure. */
#define ORACLE_CORE_HEAP_CAP ((size_t)32 << 20)
/* Module 10: feeds `doc` with (req, n, final) operations (req == INT_MIN:
 * XML_Parse) and traces the input buffer state after each (oracle_core.c);
 * with error_positions, also the position read after each failed one
 * (module 12, D-038). */
int oracle_buffer_run(const unsigned char *doc, size_t len, const int *ops,
                      size_t nops, int ns, int deferral, int error_positions,
                      unsigned char **out, size_t *out_len);
int oracle_core_run(const unsigned char *prog, size_t len, unsigned char **out,
                    size_t *out_len);
/* poolBytesToAllocateFor and offsetof(BLOCK, s). */
size_t oracle_pool_bytes_to_allocate_for(int block_size);
size_t oracle_block_header_size(void);
/* hash() of xmlparse.c for a parser with this 16-byte salt; *ulong_bits is
 * the width of the C `unsigned long` result. */
unsigned long long oracle_parser_hash(const unsigned char salt[16],
                                      const char *key, size_t len,
                                      int *ulong_bits);

const char *oracle_expat_version(void);
const char *oracle_error_string(int code);

#endif
