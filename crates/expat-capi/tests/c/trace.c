/* Prints the events, call results, error codes and positions of every
 * document given on the command line, through the expat.h functions that
 * expat-capi implements. scripts/capi-check.sh builds this file once
 * against the vendored libexpat and once against expat-capi and compares
 * the two outputs. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "expat.h"

static XML_Parser g_parser;

static void put(const char *s) {
  for (; *s; s++) {
    unsigned char c = (unsigned char)*s;
    if (c < 0x20 || c == '\\' || c >= 0x7f)
      printf("\\x%02x", c);
    else
      putchar(c);
  }
}

static void putn(const char *s, int len) {
  for (int i = 0; i < len; i++) {
    unsigned char c = (unsigned char)s[i];
    if (c < 0x20 || c == '\\' || c >= 0x7f)
      printf("\\x%02x", c);
    else
      putchar(c);
  }
}

static void XMLCALL on_start(void *ud, const XML_Char *name,
                             const XML_Char **atts) {
  (void)ud;
  printf("  start ");
  put(name);
  for (int i = 0; atts[i]; i++) {
    putchar(i % 2 ? '=' : ' ');
    put(atts[i]);
  }
  printf(" [%d]\n", XML_GetSpecifiedAttributeCount(g_parser));
}

static void XMLCALL on_end(void *ud, const XML_Char *name) {
  (void)ud;
  printf("  end ");
  put(name);
  putchar('\n');
}

static void XMLCALL on_chars(void *ud, const XML_Char *s, int len) {
  (void)ud;
  printf("  chars ");
  putn(s, len);
  putchar('\n');
}

static void XMLCALL on_pi(void *ud, const XML_Char *t, const XML_Char *d) {
  (void)ud;
  printf("  pi ");
  put(t);
  putchar(' ');
  put(d);
  putchar('\n');
}

static void XMLCALL on_comment(void *ud, const XML_Char *d) {
  (void)ud;
  printf("  comment ");
  put(d);
  putchar('\n');
}

static void XMLCALL on_cdata_start(void *ud) {
  (void)ud;
  printf("  cdata(\n");
}

static void XMLCALL on_cdata_end(void *ud) {
  (void)ud;
  printf("  )cdata\n");
}

static void XMLCALL on_ns_start(void *ud, const XML_Char *p,
                                const XML_Char *u) {
  (void)ud;
  printf("  ns ");
  put(p ? p : "(null)");
  putchar(' ');
  put(u ? u : "(null)");
  putchar('\n');
}

static void XMLCALL on_ns_end(void *ud, const XML_Char *p) {
  (void)ud;
  printf("  /ns ");
  put(p ? p : "(null)");
  putchar('\n');
}

static void position(void) {
  printf(" line=%llu col=%llu byte=%lld\n",
         (unsigned long long)XML_GetCurrentLineNumber(g_parser),
         (unsigned long long)XML_GetCurrentColumnNumber(g_parser),
         (long long)XML_GetCurrentByteIndex(g_parser));
}

static void run(const char *doc, long len, long chunk, int ns, int handlers) {
  printf(" config chunk=%ld ns=%d handlers=%d\n", chunk, ns, handlers);
  g_parser = ns ? XML_ParserCreateNS(NULL, '|') : XML_ParserCreate(NULL);
  if (handlers) {
    XML_SetElementHandler(g_parser, on_start, on_end);
    XML_SetCharacterDataHandler(g_parser, on_chars);
    XML_SetProcessingInstructionHandler(g_parser, on_pi);
    XML_SetCommentHandler(g_parser, on_comment);
    XML_SetCdataSectionHandler(g_parser, on_cdata_start, on_cdata_end);
    XML_SetNamespaceDeclHandler(g_parser, on_ns_start, on_ns_end);
  }
  long off = 0;
  do {
    long n = chunk > 0 && len - off > chunk ? chunk : len - off;
    int last = off + n >= len;
    enum XML_Status st = XML_Parse(g_parser, doc + off, (int)n, last);
    off += n;
    printf(" call status=%d error=%d", (int)st, (int)XML_GetErrorCode(g_parser));
    position();
    if (st != XML_STATUS_OK)
      break;
  } while (off < len);
  const XML_Char *msg = XML_ErrorString(XML_GetErrorCode(g_parser));
  printf(" end error=%d text=%s\n", (int)XML_GetErrorCode(g_parser),
         msg ? msg : "(null)");
  XML_ParserFree(g_parser);
}

int main(int argc, char **argv) {
  printf("version %s\n", XML_ExpatVersion());
  for (int i = 1; i < argc; i++) {
    FILE *f = fopen(argv[i], "rb");
    if (! f) {
      perror(argv[i]);
      return 2;
    }
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *doc = malloc(len ? (size_t)len : 1);
    if (! doc || fread(doc, 1, (size_t)len, f) != (size_t)len) {
      fprintf(stderr, "read %s\n", argv[i]);
      return 2;
    }
    fclose(f);
    printf("doc %s\n", argv[i]);
    static const long chunks[] = {0, 1, 7};
    for (int c = 0; c < 3; c++)
      for (int ns = 0; ns < 2; ns++)
        for (int h = 0; h < 2; h++)
          run(doc, len, chunks[c], ns, h);
    free(doc);
  }
  return 0;
}
