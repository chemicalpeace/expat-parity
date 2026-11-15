/* Prints the events, call results, error codes and positions of every
 * document given on the command line, through the expat.h functions that
 * expat-capi implements. scripts/capi-check.sh builds this file once
 * against the vendored libexpat and once against expat-capi and compares
 * the two outputs. In the third run of each configuration every handler
 * also prints the position of its event. Each configuration is fed once
 * through XML_Parse and once through XML_GetBuffer and XML_ParseBuffer;
 * edge() checks the error cases of the buffer functions. Further runs
 * stop the parser from the handlers with XML_StopParser, resuming it with
 * XML_ResumeParser after each suspension; stop_edge() checks the error
 * cases of the two functions. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "expat.h"

static XML_Parser g_parser;
/* Whether each handler also prints the position of its event. */
static int g_where;
/* Every g_every-th event stops the parser, resumable when g_resumable;
 * 0 for none. */
static int g_every;
static int g_resumable;
static long g_events;

static void position(void);

static void where(void) {
  if (g_where) {
    printf("   at");
    position();
  }
  if (g_every && ++g_events % g_every == 0) {
    int st = XML_StopParser(g_parser, (XML_Bool)g_resumable);
    printf("   stop status=%d error=%d\n", st, (int)XML_GetErrorCode(g_parser));
  }
}

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
  where();
}

static void XMLCALL on_end(void *ud, const XML_Char *name) {
  (void)ud;
  printf("  end ");
  put(name);
  putchar('\n');
  where();
}

static void XMLCALL on_chars(void *ud, const XML_Char *s, int len) {
  (void)ud;
  printf("  chars ");
  putn(s, len);
  putchar('\n');
  where();
}

static void XMLCALL on_pi(void *ud, const XML_Char *t, const XML_Char *d) {
  (void)ud;
  printf("  pi ");
  put(t);
  putchar(' ');
  put(d);
  putchar('\n');
  where();
}

static void XMLCALL on_comment(void *ud, const XML_Char *d) {
  (void)ud;
  printf("  comment ");
  put(d);
  putchar('\n');
  where();
}

static void XMLCALL on_cdata_start(void *ud) {
  (void)ud;
  printf("  cdata(\n");
  where();
}

static void XMLCALL on_cdata_end(void *ud) {
  (void)ud;
  printf("  )cdata\n");
  where();
}

static void XMLCALL on_ns_start(void *ud, const XML_Char *p,
                                const XML_Char *u) {
  (void)ud;
  printf("  ns ");
  put(p ? p : "(null)");
  putchar(' ');
  put(u ? u : "(null)");
  putchar('\n');
  where();
}

static void XMLCALL on_ns_end(void *ud, const XML_Char *p) {
  (void)ud;
  printf("  /ns ");
  put(p ? p : "(null)");
  putchar('\n');
  where();
}

static void position(void) {
  printf(" line=%llu col=%llu byte=%lld\n",
         (unsigned long long)XML_GetCurrentLineNumber(g_parser),
         (unsigned long long)XML_GetCurrentColumnNumber(g_parser),
         (long long)XML_GetCurrentByteIndex(g_parser));
}

static void run(const char *doc, long len, long chunk, int ns, int handlers,
                int buffer, int every, int resumable) {
  printf(" config chunk=%ld ns=%d handlers=%d buffer=%d", chunk, ns, handlers,
         buffer);
  if (every)
    printf(" stop=%d resumable=%d", every, resumable);
  putchar('\n');
  g_where = handlers == 2;
  g_every = every;
  g_resumable = resumable;
  g_events = 0;
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
    enum XML_Status st;
    if (buffer) {
      void *space = XML_GetBuffer(g_parser, (int)n);
      if (! space) {
        printf(" getbuffer error=%d\n", (int)XML_GetErrorCode(g_parser));
        break;
      }
      if (n)
        memcpy(space, doc + off, (size_t)n);
      st = XML_ParseBuffer(g_parser, (int)n, last);
    } else
      st = XML_Parse(g_parser, doc + off, (int)n, last);
    off += n;
    printf(" call status=%d error=%d", (int)st, (int)XML_GetErrorCode(g_parser));
    position();
    if (st == XML_STATUS_SUSPENDED) {
      /* Input is refused while suspended. */
      st = XML_Parse(g_parser, "", 0, 0);
      printf(" suspended-parse status=%d error=%d\n", (int)st,
             (int)XML_GetErrorCode(g_parser));
    }
    for (int i = 0; i < 1000000 && (st == XML_STATUS_SUSPENDED ||
                                      XML_GetErrorCode(g_parser) == XML_ERROR_SUSPENDED);
         i++) {
      st = XML_ResumeParser(g_parser);
      printf(" resume status=%d error=%d", (int)st,
             (int)XML_GetErrorCode(g_parser));
      position();
    }
    if (st != XML_STATUS_OK)
      break;
  } while (off < len);
  const XML_Char *msg = XML_ErrorString(XML_GetErrorCode(g_parser));
  printf(" end error=%d text=%s\n", (int)XML_GetErrorCode(g_parser),
         msg ? msg : "(null)");
  XML_ParserFree(g_parser);
}

static void status(const char *what, int st) {
  printf("edge %s status=%d error=%d", what, st, (int)XML_GetErrorCode(g_parser));
  position();
}

static void *getbuffer(const char *what, int len) {
  void *b = XML_GetBuffer(g_parser, len);
  printf("edge %s %s error=%d", what, b ? "ptr" : "null",
         (int)XML_GetErrorCode(g_parser));
  position();
  return b;
}

static void XMLCALL on_start_reenter(void *ud, const XML_Char *name,
                                     const XML_Char **atts) {
  (void)ud;
  (void)name;
  (void)atts;
  getbuffer("inside-getbuffer", 4);
  status("inside-parsebuffer", XML_ParseBuffer(g_parser, 0, 0));
}

/* The error cases of XML_GetBuffer and XML_ParseBuffer. */
static void edge(void) {
  g_parser = XML_ParserCreate(NULL);
  status("parsebuffer-first", XML_ParseBuffer(g_parser, 0, 0));
  getbuffer("getbuffer-negative", -1);
  void *b = getbuffer("getbuffer-8", 8);
  status("parsebuffer-negative", XML_ParseBuffer(g_parser, -1, 0));
  status("parsebuffer-too-long", XML_ParseBuffer(g_parser, 1 << 20, 0));
  if (b)
    memcpy(b, "<a>", 3);
  status("parsebuffer-3", XML_ParseBuffer(g_parser, 3, 0));
  status("parse-after", XML_Parse(g_parser, "</a>", 4, 1));
  getbuffer("getbuffer-finished", 8);
  status("parsebuffer-finished", XML_ParseBuffer(g_parser, 0, 1));
  XML_ParserFree(g_parser);

  g_parser = XML_ParserCreate(NULL);
  XML_SetStartElementHandler(g_parser, on_start_reenter);
  b = getbuffer("getbuffer-16", 16);
  if (b)
    memcpy(b, "<a><b/></a>", 11);
  status("parsebuffer-reenter", XML_ParseBuffer(g_parser, 11, 1));
  XML_ParserFree(g_parser);

  /* Many small requests, so the buffer is moved and grown. */
  g_parser = XML_ParserCreate(NULL);
  status("grow-start", XML_Parse(g_parser, "<r>", 3, 0));
  for (int i = 0; i < 3000; i++) {
    int n = 1 + i % 61;
    char *p = XML_GetBuffer(g_parser, n);
    if (! p) {
      status("grow-null", 0);
      break;
    }
    memset(p, 'x', (size_t)n);
    if (XML_ParseBuffer(g_parser, i % 5 ? n : n / 2, 0) != XML_STATUS_OK) {
      status("grow-fail", 0);
      break;
    }
  }
  status("grow-end", XML_Parse(g_parser, "</r>", 4, 1));
  XML_ParserFree(g_parser);
}

static int g_inside;

static void XMLCALL on_start_stop(void *ud, const XML_Char *name,
                                  const XML_Char **atts) {
  (void)ud;
  (void)atts;
  printf("edge start ");
  put(name);
  putchar('\n');
  if (g_inside == 0) {
    status("stop-resumable", XML_StopParser(g_parser, XML_TRUE));
    status("stop-resumable-again", XML_StopParser(g_parser, XML_TRUE));
    status("resume-inside", XML_ResumeParser(g_parser));
  } else if (g_inside == 1) {
    status("stop-abort", XML_StopParser(g_parser, XML_FALSE));
    status("stop-abort-again", XML_StopParser(g_parser, XML_FALSE));
  } else if (g_inside == 2) {
    status("stop-resumable", XML_StopParser(g_parser, XML_TRUE));
    status("stop-abort-suspended", XML_StopParser(g_parser, XML_FALSE));
  }
}

/* The error cases of XML_StopParser and XML_ResumeParser. */
static void stop_edge(void) {
  for (g_inside = 0; g_inside < 3; g_inside++) {
    printf("edge case %d\n", g_inside);
    g_parser = XML_ParserCreate(NULL);
    status("stop-not-started", XML_StopParser(g_parser, XML_TRUE));
    status("resume-not-suspended", XML_ResumeParser(g_parser));
    XML_SetStartElementHandler(g_parser, on_start_stop);
    status("parse", XML_Parse(g_parser, "<a><b/>", 7, 0));
    getbuffer("getbuffer-suspended", 8);
    status("resume", XML_ResumeParser(g_parser));
    status("resume-again", XML_ResumeParser(g_parser));
    status("parse-rest", XML_Parse(g_parser, "</a>", 4, 1));
    status("stop-finished", XML_StopParser(g_parser, XML_FALSE));
    XML_ParserFree(g_parser);
  }
  /* Suspended outside a handler: stop for good, then resume. */
  g_parser = XML_ParserCreate(NULL);
  status("parse-started", XML_Parse(g_parser, "<a>", 3, 0));
  status("stop-outside", XML_StopParser(g_parser, XML_TRUE));
  status("parse-suspended", XML_Parse(g_parser, "</a>", 4, 1));
  status("stop-abort-outside", XML_StopParser(g_parser, XML_FALSE));
  status("resume-finished", XML_ResumeParser(g_parser));
  XML_ParserFree(g_parser);
  /* Suspended outside a handler, then resumed with nothing to parse. */
  g_parser = XML_ParserCreate(NULL);
  status("parse-started", XML_Parse(g_parser, "<a>", 3, 0));
  status("stop-outside", XML_StopParser(g_parser, XML_TRUE));
  status("resume-empty", XML_ResumeParser(g_parser));
  status("parse-end", XML_Parse(g_parser, "</a>", 4, 1));
  XML_ParserFree(g_parser);
  /* Suspended outside a handler after the final piece. */
  g_parser = XML_ParserCreate(NULL);
  status("parse-final", XML_Parse(g_parser, "<a/>", 4, 1));
  status("stop-after-final", XML_StopParser(g_parser, XML_TRUE));
  XML_ParserFree(g_parser);
}

int main(int argc, char **argv) {
  printf("version %s\n", XML_ExpatVersion());
  edge();
  stop_edge();
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
        for (int h = 0; h < 3; h++)
          for (int b = 0; b < 2; b++)
            run(doc, len, chunks[c], ns, h, b, 0, 0);
    /* Stopped from the handlers: suspended at every event, at every third
     * event, and aborted at the fifth. */
    static const int stops[][2] = {{1, 1}, {3, 1}, {5, 0}};
    for (int c = 0; c < 3; c += 2)
      for (int ns = 0; ns < 2; ns++)
        for (int k = 0; k < 3; k++)
          run(doc, len, chunks[c], ns, 2, k == 1, stops[k][0], stops[k][1]);
    free(doc);
  }
  return 0;
}
