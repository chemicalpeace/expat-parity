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
 * cases of the two functions. Handler sets 3 to 6 add the default
 * handler: alone, beside every other handler with internal entities
 * expanded, beside the element and comment handlers, and set to NULL
 * beside every other handler (which still turns expansion off). Handler
 * sets 7 and 8 set every handler and the default handler, without and
 * with expansion, and each handler other than the default handler passes
 * its event to the default handler with XML_DefaultCurrent. Handler sets
 * 9 to 11 add the declaration handlers: 9 with the element and character
 * data handlers and positions (unparsed entities then reach the entity
 * declaration handler), 10 with every handler, the default handler and a
 * base set with XML_SetBase, 11 with the default handler expanded and
 * XML_DefaultCurrent called from each handler. Handler sets 12 to 17
 * change the parser's settings: 12 and 13 the declaration handlers with
 * parameter entity parsing always (and a foreign DTD) or unless
 * standalone, 14 every handler with namespace triplets and the encoding
 * set to ISO-8859-1, 15 the declaration handlers with a new base set and
 * the parsing status printed at every event, 16 and 17 every handler with
 * the billion laughs or the allocation tracker limits lowered from the
 * first handler. reset_run() parses each document again after
 * XML_ParserReset, and settings_edge() checks the setting functions,
 * the properties, the feature list and the memory functions. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The property enumeration lists the limits only for XML_GE == 1, the
 * configuration of the vendored build. */
#ifndef XML_GE
#  define XML_GE 1
#endif
#include "expat.h"

static XML_Parser g_parser;
/* Whether each handler also prints the position of its event. */
static int g_where;
/* Every g_every-th event stops the parser, resumable when g_resumable;
 * 0 for none. */
static int g_every;
static int g_resumable;
static long g_events;
/* Whether each handler calls XML_DefaultCurrent, and whether it is doing
 * so now. */
static int g_current;
static int g_in_current;
/* Whether each handler sets a new base, prints the parsing status, or
 * (the first time) lowers the billion laughs (1) or allocation tracker
 * (2) limits. */
static int g_rebase;
static int g_rebase_n;
static int g_status;
static int g_limits;
static void putz(const char *s);

static void position(void);

static void where(void) {
  /* Inside the default handler called by XML_DefaultCurrent the position
   * of a piece after the first differs (see the expat-capi crate docs);
   * the call is printed without it. */
  if (g_where && ! g_in_current) {
    printf("   at");
    position();
  }
  if (g_current && ! g_in_current) {
    printf("   current\n");
    g_in_current = 1;
    XML_DefaultCurrent(g_parser);
    g_in_current = 0;
  }
  if (g_rebase) {
    char b[32];
    snprintf(b, sizeof b, "r%d/", ++g_rebase_n);
    int st = (int)XML_SetBase(g_parser, b);
    printf("   rebase status=%d ", st);
    putz(XML_GetBase(g_parser));
    putchar('\n');
  }
  if (g_status) {
    XML_ParsingStatus ps;
    XML_GetParsingStatus(g_parser, &ps);
    printf("   status %d %d\n", (int)ps.parsing, (int)ps.finalBuffer);
  }
  if (g_limits) {
    int a, b;
    double d = 0;
    unsigned long long u = 0;
    if (g_limits == 1) {
      a = XML_SetBillionLaughsAttackProtectionMaximumAmplification(g_parser,
                                                                    1.0f);
      b = XML_SetBillionLaughsAttackProtectionActivationThreshold(g_parser, 0);
      XML_GetPropertyDouble(g_parser,
                            XML_PROP_BILLION_LAUGHS_MAXIMUM_AMPLIFICATION, &d);
      XML_GetPropertyUInt64(g_parser,
                            XML_PROP_BILLION_LAUGHS_ACTIVATION_THRESHOLD,
                            (uint64_t *)&u);
    } else {
      a = XML_SetAllocTrackerMaximumAmplification(g_parser, 1.0f);
      b = XML_SetAllocTrackerActivationThreshold(g_parser, 0);
      XML_GetPropertyDouble(g_parser,
                            XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION, &d);
      XML_GetPropertyUInt64(g_parser, XML_PROP_ALLOC_TRACKER_ACTIVATION_THRESHOLD,
                            (uint64_t *)&u);
    }
    printf("   limits %d %d %g %llu\n", a, b, d, u);
    g_limits = 0;
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

static void XMLCALL on_default(void *ud, const XML_Char *s, int len) {
  (void)ud;
  printf("  default ");
  putn(s, len);
  putchar('\n');
  where();
}

static void putz(const char *s) { put(s ? s : "(null)"); }

static void XMLCALL on_xml_decl(void *ud, const XML_Char *v, const XML_Char *e,
                                int sa) {
  (void)ud;
  printf("  xmldecl ");
  putz(v);
  putchar(' ');
  putz(e);
  printf(" %d\n", sa);
  where();
}

static void XMLCALL on_doctype_start(void *ud, const XML_Char *n,
                                     const XML_Char *sys, const XML_Char *pub,
                                     int sub) {
  (void)ud;
  printf("  doctype ");
  putz(n);
  putchar(' ');
  putz(sys);
  putchar(' ');
  putz(pub);
  printf(" %d\n", sub);
  where();
}

static void XMLCALL on_doctype_end(void *ud) {
  (void)ud;
  printf("  /doctype\n");
  where();
}

static void XMLCALL on_entity(void *ud, const XML_Char *n, int pe,
                              const XML_Char *v, int vlen, const XML_Char *b,
                              const XML_Char *sys, const XML_Char *pub,
                              const XML_Char *nt) {
  (void)ud;
  printf("  entity ");
  putz(n);
  printf(" %d ", pe);
  if (v)
    putn(v, vlen);
  else
    printf("(null)");
  printf(" %d ", vlen);
  putz(b);
  putchar(' ');
  putz(sys);
  putchar(' ');
  putz(pub);
  putchar(' ');
  putz(nt);
  putchar('\n');
  where();
}

static void XMLCALL on_unparsed(void *ud, const XML_Char *n,
                                const XML_Char *b, const XML_Char *sys,
                                const XML_Char *pub, const XML_Char *nt) {
  (void)ud;
  printf("  unparsed ");
  putz(n);
  putchar(' ');
  putz(b);
  putchar(' ');
  putz(sys);
  putchar(' ');
  putz(pub);
  putchar(' ');
  putz(nt);
  putchar('\n');
  where();
}

static void XMLCALL on_notation(void *ud, const XML_Char *n,
                                const XML_Char *b, const XML_Char *sys,
                                const XML_Char *pub) {
  (void)ud;
  printf("  notation ");
  putz(n);
  putchar(' ');
  putz(b);
  putchar(' ');
  putz(sys);
  putchar(' ');
  putz(pub);
  putchar('\n');
  where();
}

static void model(const XML_Content *m) {
  printf("(%d,%d,", (int)m->type, (int)m->quant);
  putz(m->name);
  for (unsigned i = 0; i < m->numchildren; i++)
    model(&m->children[i]);
  putchar(')');
}

static void XMLCALL on_element_decl(void *ud, const XML_Char *n,
                                    XML_Content *m) {
  (void)ud;
  printf("  elementdecl ");
  putz(n);
  putchar(' ');
  model(m);
  putchar('\n');
  XML_FreeContentModel(g_parser, m);
  where();
}

static void XMLCALL on_attlist(void *ud, const XML_Char *el,
                               const XML_Char *att, const XML_Char *type,
                               const XML_Char *dflt, int req) {
  (void)ud;
  printf("  attlist ");
  putz(el);
  putchar(' ');
  putz(att);
  putchar(' ');
  putz(type);
  putchar(' ');
  putz(dflt);
  printf(" %d\n", req);
  where();
}

static void XMLCALL on_skipped(void *ud, const XML_Char *n, int pe) {
  (void)ud;
  printf("  skipped ");
  putz(n);
  printf(" %d\n", pe);
  where();
}

static void set_decl_handlers(int unparsed) {
  XML_SetXmlDeclHandler(g_parser, on_xml_decl);
  XML_SetDoctypeDeclHandler(g_parser, on_doctype_start, on_doctype_end);
  XML_SetEntityDeclHandler(g_parser, on_entity);
  if (unparsed)
    XML_SetUnparsedEntityDeclHandler(g_parser, on_unparsed);
  XML_SetNotationDeclHandler(g_parser, on_notation);
  XML_SetElementDeclHandler(g_parser, on_element_decl);
  XML_SetAttlistDeclHandler(g_parser, on_attlist);
  XML_SetSkippedEntityHandler(g_parser, on_skipped);
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
  g_where = handlers == 2 || handlers == 3 || handlers == 5 || handlers == 8 ||
            handlers == 9 || handlers == 12 || handlers == 13 ||
            handlers == 15 || handlers == 16 || handlers == 17;
  g_rebase = handlers == 15;
  g_rebase_n = 0;
  g_status = handlers == 15;
  g_limits = handlers == 16 ? 1 : handlers == 17 ? 2 : 0;
  g_current = handlers == 7 || handlers == 8 || handlers == 11;
  g_every = every;
  g_resumable = resumable;
  g_events = 0;
  g_parser = ns ? XML_ParserCreateNS(NULL, '|') : XML_ParserCreate(NULL);
  if (handlers == 1 || handlers == 2 || handlers == 4 || handlers == 6 ||
      handlers == 7 || handlers == 8 || handlers == 10 || handlers == 11 ||
      handlers == 14 || handlers == 16 || handlers == 17) {
    XML_SetElementHandler(g_parser, on_start, on_end);
    XML_SetCharacterDataHandler(g_parser, on_chars);
    XML_SetProcessingInstructionHandler(g_parser, on_pi);
    XML_SetCommentHandler(g_parser, on_comment);
    XML_SetCdataSectionHandler(g_parser, on_cdata_start, on_cdata_end);
    XML_SetNamespaceDeclHandler(g_parser, on_ns_start, on_ns_end);
  }
  if (handlers == 5) {
    XML_SetElementHandler(g_parser, on_start, on_end);
    XML_SetCommentHandler(g_parser, on_comment);
  }
  if (handlers == 9 || handlers == 12 || handlers == 13 || handlers == 15) {
    XML_SetElementHandler(g_parser, on_start, on_end);
    XML_SetCharacterDataHandler(g_parser, on_chars);
  }
  if (handlers >= 9)
    set_decl_handlers(handlers != 9);
  if (handlers == 12 || handlers == 13) {
    int pe = XML_SetParamEntityParsing(
        g_parser, handlers == 12 ? XML_PARAM_ENTITY_PARSING_ALWAYS
                                 : XML_PARAM_ENTITY_PARSING_UNLESS_STANDALONE);
    int fd = (int)XML_UseForeignDTD(g_parser, handlers == 12);
    printf(" settings pe=%d foreign=%d\n", pe, fd);
  }
  if (handlers == 14) {
    XML_SetReturnNSTriplet(g_parser, 1);
    printf(" settings encoding=%d\n",
           (int)XML_SetEncoding(g_parser, "ISO-8859-1"));
  }
  if (handlers == 10) {
    enum XML_Status bs = XML_SetBase(g_parser, "http://example.org/b/");
    printf(" base status=%d ", (int)bs);
    putz(XML_GetBase(g_parser));
    putchar('\n');
  }
  if (handlers == 3 || handlers == 5 || handlers == 7 || handlers == 10)
    XML_SetDefaultHandler(g_parser, on_default);
  else if (handlers == 4 || handlers == 8 || handlers == 11)
    XML_SetDefaultHandlerExpand(g_parser, on_default);
  else if (handlers == 6)
    XML_SetDefaultHandler(g_parser, NULL);
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
  /* Outside a handler XML_DefaultCurrent does nothing. */
  if (g_current)
    XML_DefaultCurrent(g_parser);
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

/* XML_SetBase and XML_GetBase. */
static void base_edge(void) {
  g_parser = XML_ParserCreate(NULL);
  printf("edge base-initial ");
  putz(XML_GetBase(g_parser));
  printf("\nedge setbase status=%d ", (int)XML_SetBase(g_parser, "a/b"));
  putz(XML_GetBase(g_parser));
  printf("\nedge setbase-null status=%d ", (int)XML_SetBase(g_parser, NULL));
  putz(XML_GetBase(g_parser));
  printf("\nedge setbase-null-parser status=%d\n", (int)XML_SetBase(NULL, "x"));
  XML_ParserFree(g_parser);
}

/* Parses the document, resets the parser and parses it again. */
static void reset_run(const char *doc, long len, int ns) {
  printf(" config reset ns=%d\n", ns);
  g_where = 1;
  g_current = g_rebase = g_status = g_limits = g_every = 0;
  g_parser = ns ? XML_ParserCreateNS(NULL, '|') : XML_ParserCreate(NULL);
  XML_SetReturnNSTriplet(g_parser, 1);
  XML_SetBillionLaughsAttackProtectionMaximumAmplification(g_parser, 50.0f);
  XML_SetReparseDeferralEnabled(g_parser, XML_FALSE);
  XML_SetBase(g_parser, "before/");
  for (int round = 0; round < 2; round++) {
    XML_SetElementHandler(g_parser, on_start, on_end);
    XML_SetCharacterDataHandler(g_parser, on_chars);
    XML_SetNamespaceDeclHandler(g_parser, on_ns_start, on_ns_end);
    set_decl_handlers(1);
    enum XML_Status st = XML_Parse(g_parser, doc, (int)len, 1);
    printf(" call status=%d error=%d", (int)st, (int)XML_GetErrorCode(g_parser));
    position();
    XML_Bool r = XML_ParserReset(g_parser, round ? "UTF-8" : NULL);
    double d = 0;
    XML_Bool defer = 9;
    XML_GetPropertyDouble(g_parser, XML_PROP_BILLION_LAUGHS_MAXIMUM_AMPLIFICATION,
                          &d);
    XML_GetPropertyBool(g_parser, XML_PROP_REPARSE_DEFERRAL_ENABLED, &defer);
    XML_ParsingStatus ps;
    XML_GetParsingStatus(g_parser, &ps);
    printf(" reset %d error=%d status=%d %d max=%g defer=%d base=", (int)r,
           (int)XML_GetErrorCode(g_parser), (int)ps.parsing,
           (int)ps.finalBuffer, d, (int)defer);
    putz(XML_GetBase(g_parser));
    putchar(' ');
    position();
  }
  XML_ParserFree(g_parser);
}

static void XMLCALL on_start_settings(void *ud, const XML_Char *name,
                                      const XML_Char **atts) {
  (void)ud;
  (void)name;
  (void)atts;
  printf(" inside salt=%d salt16=%d encoding=%d foreign=%d pe=%d reset=%d\n",
         XML_SetHashSalt(g_parser, 5),
         (int)XML_SetHashSalt16Bytes(g_parser, (const uint8_t *)"0123456789abcdef"),
         (int)XML_SetEncoding(g_parser, "UTF-8"),
         (int)XML_UseForeignDTD(g_parser, XML_TRUE),
         XML_SetParamEntityParsing(g_parser, XML_PARAM_ENTITY_PARSING_ALWAYS),
         (int)XML_ParserReset(g_parser, NULL));
}

static int g_mallocs;
static void *XMLCALL count_malloc(size_t n) {
  g_mallocs++;
  return malloc(n);
}
static void *XMLCALL count_realloc(void *p, size_t n) { return realloc(p, n); }
static void XMLCALL count_free(void *p) { free(p); }

static void XMLCALL on_element_keep(void *ud, const XML_Char *n,
                                    XML_Content *m) {
  (void)ud;
  int before = g_mallocs;
  printf(" model %s ", n);
  model(m);
  /* The content model came from the suite (one call before this one). */
  printf(" free\n");
  XML_MemFree(g_parser, m);
  (void)before;
}

/* The setting functions, the properties, the feature list and the memory
 * functions. */
static void settings_edge(void) {
  g_where = g_current = g_rebase = g_status = g_limits = g_every = 0;
  const XML_Feature *f = XML_GetFeatureList();
  for (; f->feature != XML_FEATURE_END; f++)
    printf("edge feature %d %s %ld\n", (int)f->feature, f->name, f->value);
  printf("edge feature-end %d\n", (int)f->feature);

  g_parser = XML_ParserCreate(NULL);
  XML_Bool b = 7;
  double d = -1;
  uint64_t u = 1;
  for (int k = 0; k <= 6; k++) {
    printf("edge prop %d getb=%d getd=%d getu=%d", k,
           (int)XML_GetPropertyBool(g_parser, (enum XML_Parser_Property)k, &b),
           (int)XML_GetPropertyDouble(g_parser, (enum XML_Parser_Property)k, &d),
           (int)XML_GetPropertyUInt64(g_parser, (enum XML_Parser_Property)k, &u));
    printf(" values %d %g %llu", (int)b, d, (unsigned long long)u);
    printf(" setb=%d setd=%d setu=%d",
           (int)XML_SetPropertyBool(g_parser, (enum XML_Parser_Property)k, XML_TRUE),
           (int)XML_SetPropertyDouble(g_parser, (enum XML_Parser_Property)k, 3.5),
           (int)XML_SetPropertyUInt64(g_parser, (enum XML_Parser_Property)k, 77));
    printf(" null=%d nullp=%d\n",
           (int)XML_GetPropertyBool(g_parser, (enum XML_Parser_Property)k, NULL),
           (int)XML_SetPropertyBool(NULL, (enum XML_Parser_Property)k, XML_TRUE));
  }
  printf("edge prop-bad setb2=%d setd-nan=%d setd-small=%d setd-1=%d\n",
         (int)XML_SetPropertyBool(g_parser, XML_PROP_REPARSE_DEFERRAL_ENABLED, 2),
         (int)XML_SetPropertyDouble(g_parser,
                                    XML_PROP_BILLION_LAUGHS_MAXIMUM_AMPLIFICATION,
                                    0.0 / 0.0),
         (int)XML_SetPropertyDouble(g_parser,
                                    XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION,
                                    0.99),
         (int)XML_SetPropertyDouble(g_parser,
                                    XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION,
                                    1.0));
  XML_GetPropertyDouble(g_parser, XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION,
                        &d);
  XML_GetPropertyUInt64(g_parser, XML_PROP_ALLOC_TRACKER_ACTIVATION_THRESHOLD,
                        &u);
  printf("edge prop-after %g %llu\n", d, (unsigned long long)u);
  /* Back to the defaults, so that the calls below can allocate. */
  XML_SetAllocTrackerMaximumAmplification(g_parser, 100.0f);
  XML_SetAllocTrackerActivationThreshold(g_parser, 67108864);
  printf("edge set blmax-nan=%d blmax-small=%d atmax-small=%d defer2=%d "
         "defer0=%d null=%d %d %d %d %d\n",
         (int)XML_SetBillionLaughsAttackProtectionMaximumAmplification(
             g_parser, 0.0f / 0.0f),
         (int)XML_SetBillionLaughsAttackProtectionMaximumAmplification(g_parser,
                                                                       0.5f),
         (int)XML_SetAllocTrackerMaximumAmplification(g_parser, 0.5f),
         (int)XML_SetReparseDeferralEnabled(g_parser, 2),
         (int)XML_SetReparseDeferralEnabled(g_parser, XML_FALSE),
         (int)XML_SetBillionLaughsAttackProtectionMaximumAmplification(NULL, 2.0f),
         (int)XML_SetBillionLaughsAttackProtectionActivationThreshold(NULL, 2),
         (int)XML_SetAllocTrackerMaximumAmplification(NULL, 2.0f),
         (int)XML_SetAllocTrackerActivationThreshold(NULL, 2),
         (int)XML_SetReparseDeferralEnabled(NULL, XML_TRUE));
  XML_ParsingStatus ps = {9, 9};
  XML_GetParsingStatus(g_parser, &ps);
  printf("edge status-initial %d %d\n", (int)ps.parsing, (int)ps.finalBuffer);
  printf("edge before salt=%d salt0=%d salt16=%d salt16-null=%d encoding=%d "
         "encoding-null=%d foreign=%d pe=%d pe9=%d\n",
         XML_SetHashSalt(g_parser, 42), XML_SetHashSalt(g_parser, 0),
         (int)XML_SetHashSalt16Bytes(g_parser, (const uint8_t *)"0123456789abcdef"),
         (int)XML_SetHashSalt16Bytes(g_parser, NULL),
         (int)XML_SetEncoding(g_parser, "US-ASCII"),
         (int)XML_SetEncoding(g_parser, NULL),
         (int)XML_UseForeignDTD(g_parser, XML_FALSE),
         XML_SetParamEntityParsing(g_parser, XML_PARAM_ENTITY_PARSING_NEVER),
         XML_SetParamEntityParsing(g_parser, (enum XML_ParamEntityParsing)9));
  printf("edge null salt=%d salt16=%d encoding=%d foreign=%d pe=%d reset=%d\n",
         XML_SetHashSalt(NULL, 1),
         (int)XML_SetHashSalt16Bytes(NULL, (const uint8_t *)"0123456789abcdef"),
         (int)XML_SetEncoding(NULL, "UTF-8"), (int)XML_UseForeignDTD(NULL, 1),
         XML_SetParamEntityParsing(NULL, XML_PARAM_ENTITY_PARSING_ALWAYS),
         (int)XML_ParserReset(NULL, NULL));
  XML_SetReturnNSTriplet(NULL, 1);
  XML_GetParsingStatus(NULL, &ps);
  status("partial", XML_Parse(g_parser, "<a>", 3, 0));
  XML_GetParsingStatus(g_parser, &ps);
  printf("edge status-partial %d %d\n", (int)ps.parsing, (int)ps.finalBuffer);
  printf("edge busy salt=%d salt16=%d encoding=%d foreign=%d pe=%d\n",
         XML_SetHashSalt(g_parser, 42),
         (int)XML_SetHashSalt16Bytes(g_parser, (const uint8_t *)"0123456789abcdef"),
         (int)XML_SetEncoding(g_parser, "US-ASCII"),
         (int)XML_UseForeignDTD(g_parser, XML_TRUE),
         XML_SetParamEntityParsing(g_parser, XML_PARAM_ENTITY_PARSING_ALWAYS));
  XML_SetElementHandler(g_parser, on_start_settings, NULL);
  status("inside", XML_Parse(g_parser, "<b/></a>", 8, 1));
  XML_GetParsingStatus(g_parser, &ps);
  printf("edge status-final %d %d\n", (int)ps.parsing, (int)ps.finalBuffer);
  printf("edge finished salt=%d encoding=%d foreign=%d pe=%d\n",
         XML_SetHashSalt(g_parser, 42),
         (int)XML_SetEncoding(g_parser, "US-ASCII"),
         (int)XML_UseForeignDTD(g_parser, XML_TRUE),
         XML_SetParamEntityParsing(g_parser, XML_PARAM_ENTITY_PARSING_ALWAYS));
  /* An encoding set before the first call is used, and kept by a reset
   * only when passed again. */
  printf("edge reset=%d encoding=%d\n", (int)XML_ParserReset(g_parser, NULL),
         (int)XML_SetEncoding(g_parser, "US-ASCII"));
  status("ascii", XML_Parse(g_parser, "<a>\xc3\xa9</a>", 9, 1));
  printf("edge reset=%d\n", (int)XML_ParserReset(g_parser, "ISO-8859-1"));
  XML_SetCharacterDataHandler(g_parser, on_chars);
  status("latin1", XML_Parse(g_parser, "<a>\xe9</a>", 8, 1));
  printf("edge reset=%d\n", (int)XML_ParserReset(g_parser, "no-such"));
  status("unknown", XML_Parse(g_parser, "<a/>", 4, 1));
  void *m = XML_MemMalloc(g_parser, 16);
  m = XML_MemRealloc(g_parser, m, 32);
  printf("edge mem %d %d %d\n", m != NULL, XML_MemMalloc(NULL, 1) == NULL,
         XML_MemRealloc(NULL, m, 1) == NULL);
  XML_MemFree(g_parser, m);
  XML_MemFree(NULL, NULL);
  XML_FreeContentModel(NULL, NULL);
  XML_ParserFree(g_parser);

  XML_Memory_Handling_Suite suite = {count_malloc, count_realloc, count_free};
  g_parser = XML_ParserCreate_MM(NULL, &suite, "|");
  XML_SetElementDeclHandler(g_parser, on_element_keep);
  XML_SetStartNamespaceDeclHandler(g_parser, on_ns_start);
  status("mm", XML_Parse(g_parser,
                         "<!DOCTYPE p:a [<!ELEMENT p:a (b|c)*>]>"
                         "<p:a xmlns:p='u'/>",
                         56, 1));
  printf("edge mm-malloc-used %d\n", g_mallocs > 0);
  XML_ParserFree(g_parser);
  g_parser = XML_ParserCreate_MM(NULL, NULL, NULL);
  status("mm-default", XML_Parse(g_parser, "<p:a/>", 6, 1));
  XML_ParserFree(g_parser);
}

int main(int argc, char **argv) {
  printf("version %s\n", XML_ExpatVersion());
  edge();
  base_edge();
  stop_edge();
  settings_edge();
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
        for (int h = 0; h < 18; h++)
          for (int b = 0; b < 2; b++)
            run(doc, len, chunks[c], ns, h, b, 0, 0);
    /* Stopped from the handlers: suspended at every event, at every third
     * event, and aborted at the fifth. */
    static const int stops[][2] = {{1, 1}, {3, 1}, {5, 0}};
    for (int c = 0; c < 3; c += 2)
      for (int ns = 0; ns < 2; ns++)
        for (int k = 0; k < 3; k++)
          run(doc, len, chunks[c], ns, 2, k == 1, stops[k][0], stops[k][1]);
    /* The default handler alone, and the declaration handlers, suspended
     * at every event. */
    for (int ns = 0; ns < 2; ns++) {
      run(doc, len, 0, ns, 3, 0, 1, 1);
      run(doc, len, 0, ns, 9, 0, 1, 1);
      reset_run(doc, len, ns);
    }
    free(doc);
  }
  return 0;
}
