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
  case ORACLE_TABLE_BIG2:
    return &big2_encoding;
  case ORACLE_TABLE_BIG2_NS:
    return &big2_encoding_ns;
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

/* ---- siphash.h (header-only, all static) -------------------------------- */
#include "siphash.h"

unsigned long long
oracle_siphash24(const unsigned char key[16], const unsigned char *data,
                 size_t len, size_t split) {
  struct siphash state;
  struct sipkey k;
  sip_tokey(&k, key);
  sip24_init(&state, &k);
  if (split == 0)
    split = len ? len : 1;
  for (size_t off = 0; off < len; off += split) {
    size_t n = len - off < split ? len - off : split;
    sip24_update(&state, data + off, n);
  }
  if (len == 0)
    sip24_update(&state, data, 0);
  return sip24_final(&state);
}

int
oracle_sip24_valid(void) {
  return sip24_valid();
}

/* ---- xmltok_impl.c scanners through the ENCODING vtable ------------------
 * `encv` is an `ENCODING *` from oracle_encoding() or oracle_unknown_enc().
 * Offsets are relative to `buf`; -1 means "pointer not written by C". */

const void *
oracle_encoding(int id) {
  const struct normal_encoding *e = table_by_id(id);
  return e ? &e->enc : NULL;
}

static long long
offset_or_none(const char *p, const char *buf) {
  return p ? (long long)(p - buf) : -1;
}

int
oracle_tok_scan(const void *encv, int which, const char *buf, size_t len, size_t ptr,
                long long *next) {
  const ENCODING *enc = (const ENCODING *)encv;
  const char *nextp = NULL;
  int tok;
  if (which < XML_N_STATES)
    tok = enc->scanners[which](enc, buf + ptr, buf + len, &nextp);
  else
    tok = enc->literalScanners[which - XML_N_STATES](enc, buf + ptr,
                                                     buf + len, &nextp);
  *next = offset_or_none(nextp, buf);
  return tok;
}

int
oracle_tok_is_public_id(const void *encv, const char *buf, size_t len, size_t ptr,
                        long long *bad) {
  const ENCODING *enc = (const ENCODING *)encv;
  const char *badp = NULL;
  int ok = enc->isPublicId(enc, buf + ptr, buf + len, &badp);
  *bad = offset_or_none(badp, buf);
  return ok;
}

int
oracle_tok_get_atts(const void *encv, const char *buf, size_t ptr, int atts_max,
                    long long *out) {
  const ENCODING *enc = (const ENCODING *)encv;
  ATTRIBUTE atts[64];
  int n, i;
  if (atts_max < 0 || atts_max > 64)
    return -1;
  for (i = 0; i < atts_max; i++) {
    atts[i].name = buf;
    atts[i].valuePtr = buf;
    atts[i].valueEnd = buf;
    atts[i].normalized = 0;
  }
  n = enc->getAtts(enc, buf + ptr, atts_max, atts);
  for (i = 0; i < atts_max; i++) {
    out[4 * i] = atts[i].name - buf;
    out[4 * i + 1] = atts[i].valuePtr - buf;
    out[4 * i + 2] = atts[i].valueEnd - buf;
    out[4 * i + 3] = atts[i].normalized;
  }
  return n;
}

int
oracle_tok_char_ref_number(const void *encv, const char *buf, size_t ptr) {
  const ENCODING *enc = (const ENCODING *)encv;
  return enc->charRefNumber(enc, buf + ptr);
}

int
oracle_tok_predefined_entity_name(const void *encv, const char *buf, size_t len,
                                  size_t ptr) {
  const ENCODING *enc = (const ENCODING *)encv;
  return enc->predefinedEntityName(enc, buf + ptr, buf + len);
}

int
oracle_tok_name_matches_ascii(const void *encv, const char *buf, size_t len,
                              size_t ptr, const char *ascii) {
  const ENCODING *enc = (const ENCODING *)encv;
  return enc->nameMatchesAscii(enc, buf + ptr, buf + len, ascii);
}

long long
oracle_tok_name_length(const void *encv, const char *buf, size_t ptr) {
  const ENCODING *enc = (const ENCODING *)encv;
  return enc->nameLength(enc, buf + ptr);
}

long long
oracle_tok_skip_s(const void *encv, const char *buf, size_t ptr) {
  const ENCODING *enc = (const ENCODING *)encv;
  return enc->skipS(enc, buf + ptr) - buf;
}

void
oracle_tok_update_position(const void *encv, const char *buf, size_t len, size_t ptr,
                           unsigned long long pos[2]) {
  const ENCODING *enc = (const ENCODING *)encv;
  POSITION p;
  p.lineNumber = (XML_Size)pos[0];
  p.columnNumber = (XML_Size)pos[1];
  enc->updatePosition(enc, buf + ptr, buf + len, &p);
  pos[0] = p.lineNumber;
  pos[1] = p.columnNumber;
}

/* ---- converters (utf8Convert / utf16Convert) ----------------------------- */

int
oracle_convert(const void *encv, int to_utf16, const char *from,
               size_t from_len, void *to, size_t to_len, size_t *read,
               size_t *written) {
  const ENCODING *enc = (const ENCODING *)encv;
  const char *f = from;
  int res;
  if (to_utf16) {
    unsigned short *t0 = (unsigned short *)to;
    unsigned short *t = t0;
    res = XmlUtf16Convert(enc, &f, from + from_len, &t, t0 + to_len);
    *written = (size_t)(t - t0);
  } else {
    char *t0 = (char *)to;
    char *t = t0;
    res = XmlUtf8Convert(enc, &f, from + from_len, &t, t0 + to_len);
    *written = (size_t)(t - t0);
  }
  *read = (size_t)(f - from);
  return res;
}

size_t
oracle_trim_utf8(const char *from, size_t len) {
  const char *lim = from + len;
  oracle_i_trim_to_complete_utf8_characters(from, &lim);
  return (size_t)(lim - from);
}

/* ---- unknown encodings (XmlInitUnknownEncoding) ------------------------- */

/* The converter used by the differential tests: the code point of a
   sequence is tab[(first byte << 8) | (xor of the other bytes)], where the
   sequence length comes from the map. Mirrored by expat_difftest. */
struct oracle_conv {
  int map[256];
  int tab[65536];
};

struct oracle_unknown {
  struct oracle_conv *conv;
  void *mem;
  ENCODING *enc;
};

static int XMLCALL
oracle_table_convert(void *data, const char *s) {
  const struct oracle_conv *c = (const struct oracle_conv *)data;
  unsigned char b0 = (unsigned char)s[0];
  int n = -c->map[b0];
  unsigned x = 0;
  int i;
  for (i = 1; i < n; i++)
    x ^= (unsigned char)s[i];
  return c->tab[(b0 << 8) | x];
}

void
oracle_unknown_free(void *h) {
  struct oracle_unknown *u = (struct oracle_unknown *)h;
  if (u == NULL)
    return;
  free(u->conv);
  free(u->mem);
  free(u);
}

void *
oracle_unknown_new(const int map[256], const int *tab, int ns) {
  struct oracle_unknown *u = calloc(1, sizeof *u);
  if (u == NULL)
    return NULL;
  u->conv = malloc(sizeof *u->conv);
  u->mem = malloc((size_t)oracle_i_XmlSizeOfUnknownEncoding());
  if (u->conv == NULL || u->mem == NULL) {
    oracle_unknown_free(u);
    return NULL;
  }
  memcpy(u->conv->map, map, sizeof u->conv->map);
  if (tab)
    memcpy(u->conv->tab, tab, sizeof u->conv->tab);
  if (ns)
    u->enc = oracle_i_XmlInitUnknownEncodingNS(
        u->mem, map, tab ? oracle_table_convert : NULL, u->conv);
  else
    u->enc = oracle_i_XmlInitUnknownEncoding(
        u->mem, map, tab ? oracle_table_convert : NULL, u->conv);
  if (u->enc == NULL) {
    oracle_unknown_free(u);
    return NULL;
  }
  return u;
}

const void *
oracle_unknown_enc(const void *h) {
  return ((const struct oracle_unknown *)h)->enc;
}
