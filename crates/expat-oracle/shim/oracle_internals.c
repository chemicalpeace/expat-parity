/* Build shim (not part of upstream): white-box access to xmltok.c internals.
 *
 * The Rust port is built bottom-up, so leaf modules (byte-type tables, name
 * character predicates, UTF-8 validity checks, ...) must be compared against
 * the C implementation *before* the public parser exists. Those leaves are
 * `static` in xmltok.c, so this translation unit textually includes the
 * unmodified vendored xmltok.c and exports thin accessors.
 *
 * The exported symbols of xmltok.c are renamed with a prefix so this second
 * copy does not collide with the copy linked into the full parser.
 * Nothing in vendor/ is modified. See docs/DECISIONS.md (D-009).
 */
#define XmlGetUtf16InternalEncoding oracle_i_XmlGetUtf16InternalEncoding
#define XmlGetUtf16InternalEncodingNS oracle_i_XmlGetUtf16InternalEncodingNS
#define XmlGetUtf8InternalEncoding oracle_i_XmlGetUtf8InternalEncoding
#define XmlGetUtf8InternalEncodingNS oracle_i_XmlGetUtf8InternalEncodingNS
#define XmlInitEncoding oracle_i_XmlInitEncoding
#define XmlInitEncodingNS oracle_i_XmlInitEncodingNS
#define XmlInitUnknownEncoding oracle_i_XmlInitUnknownEncoding
#define XmlInitUnknownEncodingNS oracle_i_XmlInitUnknownEncodingNS
#define XmlParseXmlDecl oracle_i_XmlParseXmlDecl
#define XmlParseXmlDeclNS oracle_i_XmlParseXmlDeclNS
#define XmlSizeOfUnknownEncoding oracle_i_XmlSizeOfUnknownEncoding
#define XmlUtf16Encode oracle_i_XmlUtf16Encode
#define XmlUtf8Encode oracle_i_XmlUtf8Encode
#define _INTERNAL_trim_to_complete_utf8_characters                             \
  oracle_i_trim_to_complete_utf8_characters

#include "xmltok.c"

#include "oracle.h"

static const struct normal_encoding *
table_by_id(int id) {
  switch (id) {
  case ORACLE_TABLE_UTF8:
    return &utf8_encoding;
  case ORACLE_TABLE_UTF8_NS:
    return &utf8_encoding_ns;
  case ORACLE_TABLE_INTERNAL_UTF8:
    return &internal_utf8_encoding;
  case ORACLE_TABLE_INTERNAL_UTF8_NS:
    return &internal_utf8_encoding_ns;
  case ORACLE_TABLE_LATIN1:
    return &latin1_encoding;
  case ORACLE_TABLE_LATIN1_NS:
    return &latin1_encoding_ns;
  case ORACLE_TABLE_ASCII:
    return &ascii_encoding;
  case ORACLE_TABLE_ASCII_NS:
    return &ascii_encoding_ns;
  case ORACLE_TABLE_LITTLE2:
    return &little2_encoding;
  case ORACLE_TABLE_LITTLE2_NS:
    return &little2_encoding_ns;
  default:
    return NULL;
  }
}

int
oracle_type_table(int id, unsigned char out[256]) {
  const struct normal_encoding *e = table_by_id(id);
  if (e == NULL)
    return -1;
  memcpy(out, e->type, 256);
  return 0;
}

int
oracle_ucs2_naming(int which, unsigned int hi, unsigned int lo) {
  /* which: 0 = name start (nmstrtPages), 1 = name char (namePages) */
  if (hi > 0xFF || lo > 0xFF)
    return -1;
  if (which == 0)
    return UCS2_GET_NAMING(nmstrtPages, hi, lo) ? 1 : 0;
  return UCS2_GET_NAMING(namePages, hi, lo) ? 1 : 0;
}

int
oracle_utf8_predicate(int pred, const unsigned char *p, size_t len) {
  /* The C predicates read exactly 2, 3 or 4 bytes; the caller guarantees
     length. Returns -1 when the buffer is too short. */
  const char *c = (const char *)p;
  switch (pred) {
  case ORACLE_PRED_IS_NAME2:
    return len < 2 ? -1 : (utf8_isName2(NULL, c) ? 1 : 0);
  case ORACLE_PRED_IS_NAME3:
    return len < 3 ? -1 : (utf8_isName3(NULL, c) ? 1 : 0);
  case ORACLE_PRED_IS_NMSTRT2:
    return len < 2 ? -1 : (utf8_isNmstrt2(NULL, c) ? 1 : 0);
  case ORACLE_PRED_IS_NMSTRT3:
    return len < 3 ? -1 : (utf8_isNmstrt3(NULL, c) ? 1 : 0);
  case ORACLE_PRED_IS_INVALID2:
    return len < 2 ? -1 : (utf8_isInvalid2(NULL, c) ? 1 : 0);
  case ORACLE_PRED_IS_INVALID3:
    return len < 3 ? -1 : (utf8_isInvalid3(NULL, c) ? 1 : 0);
  case ORACLE_PRED_IS_INVALID4:
    return len < 4 ? -1 : (utf8_isInvalid4(NULL, c) ? 1 : 0);
  default:
    return -1;
  }
}

int
oracle_utf8_encode(int c, unsigned char out[4]) {
  return oracle_i_XmlUtf8Encode(c, (char *)out);
}

int
oracle_utf16_encode(int c, unsigned short out[2]) {
  return oracle_i_XmlUtf16Encode(c, out);
}
