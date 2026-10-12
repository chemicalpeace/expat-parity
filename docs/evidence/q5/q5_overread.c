/* Q-5 evidence: does libexpat 2.9.0's unknown_toUtf8, run by toAscii in
 * XmlParseXmlDecl, read past the end of the data the application passed?
 *
 * The protocol encoding "x-q5" is unknown, so the unknown-encoding handler
 * installs a map before the XML declaration is parsed (initializeEncoding ->
 * handleUnknownEncoding). Map: ASCII identity, 0xC3 lead of 2, 0xA9 and 0xBE
 * leads of 4, every other byte malformed (-1). 0xBE is the byte ASan fills
 * fresh heap blocks with (malloc_fill_byte), i.e. "uninitialised" memory in
 * an ASan build. The converter reads every byte of the sequence the map
 * announces, logs where it was called, gives 'I' (U+0049, as the converter of seed 0x52f43d23123766f6 does) for C3 A9 and -1 for
 * anything else (no output, so unknown_toUtf8 goes on).
 *
 * Mode "parsebuffer": the document is written into XML_GetBuffer's buffer,
 * and the rest of that buffer is filled with 0xBE before XML_ParseBuffer;
 * reads at offsets >= len are reads of bytes the application did not pass.
 * Mode "parse": XML_Parse copies the document into the parser's own
 * buffer; the bytes after it are uninitialised heap memory.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "expat.h"

static const char *g_base; /* start of the data the parser holds */
static int g_len;          /* length of the document */
static long g_calls, g_past;
static long g_max_off = -1;
static long g_first_lead;

static int seq_len(unsigned char b) {
  return b == 0xC3 ? 2 : (b == 0xA9 || (b == 0xBE && !getenv("Q5_NO_BE"))) ? 4 : 1;
}

static int XMLCALL convert(void *data, const char *s) {
  (void)data;
  unsigned char b0 = (unsigned char)s[0];
  if (!g_base) g_base = s - g_first_lead; /* XML_Parse: first call is the tokenizer's, at the first lead byte */
  int n = seq_len(b0);
  unsigned sum = 0;
  for (int i = 0; i < n; i++)
    sum += (unsigned char)s[i]; /* the read under test */
  long off = (long)(s - g_base);
  g_calls++;
  if (off + n > g_len) g_past++;
  if (off + n - 1 > g_max_off) g_max_off = off + n - 1;
  if (g_calls <= 12 || (g_calls % 1000) == 0)
    fprintf(stderr, "  convert #%ld at offset %ld (lead %02X, reads %d bytes up to offset %ld; doc len %d)%s\n",
            g_calls, off, b0, n, off + n - 1, g_len,
            off + n > g_len ? "  <-- past the document" : "");
  if (b0 == 0xC3 && (unsigned char)s[1] == 0xA9) return 0x49;
  return (int)(sum & 0) - 1;
}

static int XMLCALL unknown(void *data, const XML_Char *name, XML_Encoding *info) {
  (void)data; (void)name;
  for (int i = 0; i < 256; i++) info->map[i] = i < 128 ? i : -1;
  info->map[0xC3] = -2;
  info->map[0xA9] = -4;
  if (!getenv("Q5_NO_BE")) /* Q5_NO_BE=1: a map without the ASan fill byte as lead */
    info->map[0xBE] = -4;
  info->convert = convert;
  info->data = NULL;
  info->release = NULL;
  return XML_STATUS_OK;
}

int main(int argc, char **argv) {
  const char *mode = argc > 1 ? argv[1] : "parse";
  int which = argc > 2 ? atoi(argv[2]) : 0;
  static const char *docs[] = {"<?xml \xC3\xA9?>", "<?xml no\xC3\xA9 ?>", "<?xml \xC3\xA9?><a/>"};
  const char *doc = docs[which];
  g_len = (int)strlen(doc);
  g_first_lead = (long)(strchr(doc, '\xC3') - doc);
  XML_Parser p = XML_ParserCreate("x-q5");
  XML_SetUnknownEncodingHandler(p, unknown, NULL);
  enum XML_Status st;
  if (strcmp(mode, "parsebuffer") == 0) {
    char *buf = XML_GetBuffer(p, 4096);
    memset(buf, 0xBE, 4096);
    memcpy(buf, doc, (size_t)g_len);
    g_base = buf;
    st = XML_ParseBuffer(p, g_len, 1);
  } else {
    /* XML_Parse copies doc into the parser's buffer; offsets are taken
     * relative to the first convert call (the tokenizer's, at the C3). */
    g_base = NULL;
    st = XML_Parse(p, doc, g_len, 1);
  }
  fprintf(stderr, "doc %d (%d bytes), mode %s: status %d error %d (%s) byte index %ld line %lu column %lu; convert calls %ld, calls reading past the document %ld, max offset read %ld\n",
          which, g_len, mode, (int)st, (int)XML_GetErrorCode(p), XML_ErrorString(XML_GetErrorCode(p)),
          (long)XML_GetCurrentByteIndex64(p), (unsigned long)XML_GetCurrentLineNumber64(p),
          (unsigned long)XML_GetCurrentColumnNumber64(p), g_calls, g_past, g_max_off);
  XML_ParserFree(p);
  return 0;
}
