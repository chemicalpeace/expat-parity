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
};

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

/* Runs the full parser and writes a canonical event trace (see
 * docs/ARCHITECTURE.md, "Trace format") into a malloc'd buffer.
 * Returns 0 on success (the parse itself may have failed; that is part of
 * the trace), -1 on allocation failure in the shim. */
int oracle_parse_trace(const unsigned char *doc, size_t len,
                       const struct oracle_options *opts, unsigned char **out,
                       size_t *out_len);
void oracle_free(void *p);

const char *oracle_expat_version(void);

#endif
