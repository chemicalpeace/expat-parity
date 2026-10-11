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
  ORACLE_TABLE_LITTLE2_NS = 9
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

/* xmltok_impl.c through the ENCODING vtable of table `table` (an
 * ORACLE_TABLE_* id). `which`: 0 prolog, 1 content, 2 CDATA section,
 * 3 ignore section, 4 attribute value, 5 entity value. Offsets are relative
 * to `buf`; -1 means the C function did not write the pointer. Functions
 * without a `len` read until a terminator, like the C ones: only call them on
 * input that satisfies their precondition. */
int oracle_tok_scan(int table, int which, const char *buf, size_t len,
                    size_t ptr, long long *next);
int oracle_tok_is_public_id(int table, const char *buf, size_t len, size_t ptr,
                            long long *bad);
int oracle_tok_get_atts(int table, const char *buf, size_t ptr, int atts_max,
                        long long *out);
int oracle_tok_char_ref_number(int table, const char *buf, size_t ptr);
int oracle_tok_predefined_entity_name(int table, const char *buf, size_t len,
                                      size_t ptr);
int oracle_tok_name_matches_ascii(int table, const char *buf, size_t len,
                                  size_t ptr, const char *ascii);
long long oracle_tok_name_length(int table, const char *buf, size_t ptr);
long long oracle_tok_skip_s(int table, const char *buf, size_t ptr);
void oracle_tok_update_position(int table, const char *buf, size_t len,
                                size_t ptr, unsigned long long pos[2]);

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
