/* Build shim (not part of upstream): canonical event trace of a libexpat
 * parse. The trace format is the definition of "equal" used by the
 * differential harness; it is specified in docs/ARCHITECTURE.md and must be
 * kept in sync with tests/src/trace.rs.
 *
 * Doing the tracing in C keeps the FFI surface to a single call that returns
 * a byte buffer: no Rust callbacks are invoked from C.
 */
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <stdint.h>

#include "expat.h"
#include "oracle.h"

/* Fixed hash salt so libexpat never draws entropy. The salt seeds the
   internal SipHash tables; their slot order shows only in the context string
   of external entity references (getContext), which the trace does not
   record (D-021). */
static const uint8_t ORACLE_HASH_SALT[16]
    = {0x65, 0x78, 0x70, 0x61, 0x74, 0x2d, 0x72, 0x73,
       0x2d, 0x6f, 0x72, 0x61, 0x63, 0x6c, 0x65, 0x21};

struct buf {
  unsigned char *data;
  size_t len;
  size_t cap;
  int failed;
};

struct tracer {
  struct buf out;
  struct buf text; /* pending coalesced character data */
  int in_text;
};

static void
buf_push(struct buf *b, const void *src, size_t n) {
  if (b->failed)
    return;
  if (b->len + n > b->cap) {
    size_t cap = b->cap ? b->cap : 256;
    while (cap < b->len + n) {
      if (cap > ((size_t)-1) / 2) {
        b->failed = 1;
        return;
      }
      cap *= 2;
    }
    unsigned char *d = realloc(b->data, cap);
    if (d == NULL) {
      b->failed = 1;
      return;
    }
    b->data = d;
    b->cap = cap;
  }
  memcpy(b->data + b->len, src, n);
  b->len += n;
}

static void
buf_str(struct buf *b, const char *s) {
  buf_push(b, s, strlen(s));
}

static int
needs_escape(unsigned char c) {
  return c <= 0x20 || c >= 0x7f || c == '%' || c == '~' || c == '('
         || c == ')' || c == ',';
}

/* Escaped field: bytes outside a safe printable set become %HH. */
static void
buf_field_n(struct buf *b, const char *s, size_t n) {
  static const char hex[] = "0123456789ABCDEF";
  buf_push(b, " ", 1);
  if (s == NULL) {
    buf_push(b, "~", 1);
    return;
  }
  for (size_t i = 0; i < n; i++) {
    unsigned char c = (unsigned char)s[i];
    if (needs_escape(c)) {
      char e[3] = {'%', hex[c >> 4], hex[c & 0xF]};
      buf_push(b, e, 3);
    } else {
      buf_push(b, &c, 1);
    }
  }
}

static void
buf_field(struct buf *b, const char *s) {
  buf_field_n(b, s, s ? strlen(s) : 0);
}

static void
buf_int(struct buf *b, long long v) {
  char tmp[32];
  int n = snprintf(tmp, sizeof tmp, " %lld", v);
  if (n > 0)
    buf_push(b, tmp, (size_t)n);
}

static void
buf_uint(struct buf *b, unsigned long long v) {
  char tmp[32];
  int n = snprintf(tmp, sizeof tmp, " %llu", v);
  if (n > 0)
    buf_push(b, tmp, (size_t)n);
}

static void
flush_text(struct tracer *t) {
  if (! t->in_text)
    return;
  buf_str(&t->out, "text");
  buf_field_n(&t->out, (const char *)t->text.data, t->text.len);
  buf_push(&t->out, "\n", 1);
  if (t->text.failed)
    t->out.failed = 1;
  t->text.len = 0;
  t->in_text = 0;
}

static struct tracer *
begin(void *ud, const char *tag) {
  struct tracer *t = ud;
  flush_text(t);
  buf_str(&t->out, tag);
  return t;
}

static void
end_line(struct tracer *t) {
  buf_push(&t->out, "\n", 1);
}

static void XMLCALL
on_xmldecl(void *ud, const XML_Char *version, const XML_Char *encoding,
           int standalone) {
  struct tracer *t = begin(ud, "xmldecl");
  buf_field(&t->out, version);
  buf_field(&t->out, encoding);
  buf_int(&t->out, standalone);
  end_line(t);
}

static void XMLCALL
on_doctype(void *ud, const XML_Char *name, const XML_Char *sysid,
           const XML_Char *pubid, int has_internal_subset) {
  struct tracer *t = begin(ud, "doctype");
  buf_field(&t->out, name);
  buf_field(&t->out, sysid);
  buf_field(&t->out, pubid);
  buf_int(&t->out, has_internal_subset);
  end_line(t);
}

static void XMLCALL
on_enddoctype(void *ud) {
  end_line(begin(ud, "enddoctype"));
}

struct start_ctx {
  struct tracer *t;
  XML_Parser p;
};

static void XMLCALL
on_start(void *ud, const XML_Char *name, const XML_Char **atts) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "start");
  buf_field(&t->out, name);
  buf_int(&t->out, XML_GetSpecifiedAttributeCount(c->p));
  end_line(t);
  for (size_t i = 0; atts[i] != NULL; i += 2) {
    buf_str(&t->out, "attr");
    buf_field(&t->out, atts[i]);
    buf_field(&t->out, atts[i + 1]);
    end_line(t);
  }
}

static void XMLCALL
on_end(void *ud, const XML_Char *name) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "end");
  buf_field(&t->out, name);
  end_line(t);
}

static void XMLCALL
on_text(void *ud, const XML_Char *s, int len) {
  struct start_ctx *c = ud;
  struct tracer *t = c->t;
  t->in_text = 1;
  if (len > 0)
    buf_push(&t->text, s, (size_t)len);
}

static void XMLCALL
on_pi(void *ud, const XML_Char *target, const XML_Char *data) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "pi");
  buf_field(&t->out, target);
  buf_field(&t->out, data);
  end_line(t);
}

static void XMLCALL
on_comment(void *ud, const XML_Char *data) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "comment");
  buf_field(&t->out, data);
  end_line(t);
}

static void XMLCALL
on_cdata(void *ud) {
  struct start_ctx *c = ud;
  end_line(begin(c->t, "cdata"));
}

static void XMLCALL
on_endcdata(void *ud) {
  struct start_ctx *c = ud;
  end_line(begin(c->t, "endcdata"));
}

static void XMLCALL
on_entity(void *ud, const XML_Char *name, int is_param, const XML_Char *value,
          int value_len, const XML_Char *base, const XML_Char *sysid,
          const XML_Char *pubid, const XML_Char *notation) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "entity");
  buf_field(&t->out, name);
  buf_int(&t->out, is_param);
  if (value == NULL)
    buf_field(&t->out, NULL);
  else
    buf_field_n(&t->out, value, value_len > 0 ? (size_t)value_len : 0);
  buf_field(&t->out, base);
  buf_field(&t->out, sysid);
  buf_field(&t->out, pubid);
  buf_field(&t->out, notation);
  end_line(t);
}

static void XMLCALL
on_notation(void *ud, const XML_Char *name, const XML_Char *base,
            const XML_Char *sysid, const XML_Char *pubid) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "notation");
  buf_field(&t->out, name);
  buf_field(&t->out, base);
  buf_field(&t->out, sysid);
  buf_field(&t->out, pubid);
  end_line(t);
}

static void
model(struct buf *b, const XML_Content *m, int depth) {
  char head[8];
  /* Expat bounds content model depth itself; this guard only protects the
     shim's own recursion against a hypothetical unbounded model. */
  if (depth > 100000) {
    b->failed = 1;
    return;
  }
  snprintf(head, sizeof head, "%d%d", (int)m->type, (int)m->quant);
  buf_str(b, head);
  buf_field(b, m->name);
  buf_push(b, "(", 1);
  for (unsigned int i = 0; i < m->numchildren; i++) {
    if (i > 0)
      buf_push(b, ",", 1);
    model(b, &m->children[i], depth + 1);
  }
  buf_push(b, ")", 1);
}

static void XMLCALL
on_element(void *ud, const XML_Char *name, XML_Content *m) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "element");
  buf_field(&t->out, name);
  buf_push(&t->out, " ", 1);
  model(&t->out, m, 0);
  end_line(t);
  XML_FreeContentModel(c->p, m);
}

static void XMLCALL
on_attlist(void *ud, const XML_Char *elname, const XML_Char *attname,
           const XML_Char *att_type, const XML_Char *dflt, int isrequired) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "attlist");
  buf_field(&t->out, elname);
  buf_field(&t->out, attname);
  buf_field(&t->out, att_type);
  buf_field(&t->out, dflt);
  buf_int(&t->out, isrequired);
  end_line(t);
}

static void XMLCALL
on_nsstart(void *ud, const XML_Char *prefix, const XML_Char *uri) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "nsstart");
  buf_field(&t->out, prefix);
  buf_field(&t->out, uri);
  end_line(t);
}

static void XMLCALL
on_nsend(void *ud, const XML_Char *prefix) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "nsend");
  buf_field(&t->out, prefix);
  end_line(t);
}

static void XMLCALL
on_skipped(void *ud, const XML_Char *name, int is_param) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "skipped");
  buf_field(&t->out, name);
  buf_int(&t->out, is_param);
  end_line(t);
}

static void XMLCALL
on_xmldecl_w(void *ud, const XML_Char *v, const XML_Char *e, int s) {
  on_xmldecl(((struct start_ctx *)ud)->t, v, e, s);
}

static void XMLCALL
on_doctype_w(void *ud, const XML_Char *n, const XML_Char *s,
             const XML_Char *p, int h) {
  on_doctype(((struct start_ctx *)ud)->t, n, s, p, h);
}

static void XMLCALL
on_enddoctype_w(void *ud) {
  on_enddoctype(((struct start_ctx *)ud)->t);
}

/* Feeds n bytes, through XML_Parse or through the buffer API with a
   larger buffer request than the data (so the request size, which the
   reparse-deferral heuristic reads, differs from the chunk). */
static enum XML_Status
feed(XML_Parser p, const unsigned char *s, size_t n, int final,
     int buffer_api) {
  if (! buffer_api)
    return XML_Parse(p, (const char *)s, (int)n, final);
  int req = n <= (size_t)(0x7fffffff - 3) / 2 ? (int)(2 * n + 3) : (int)n;
  void *b = XML_GetBuffer(p, req);
  if (b == NULL)
    return XML_STATUS_ERROR;
  if (n > 0)
    memcpy(b, s, n);
  return XML_ParseBuffer(p, (int)n, final);
}

int
oracle_parse_trace(const unsigned char *doc, size_t len,
                   const struct oracle_options *opts, unsigned char **out,
                   size_t *out_len) {
  struct tracer t;
  struct start_ctx ctx;
  XML_Parser p;
  enum XML_Status st = XML_STATUS_OK;

  memset(&t, 0, sizeof t);
  *out = NULL;
  *out_len = 0;

  if (opts->ns_separator != 0)
    p = XML_ParserCreateNS(opts->encoding, (XML_Char)opts->ns_separator);
  else
    p = XML_ParserCreate(opts->encoding);
  if (p == NULL)
    return -1;

  ctx.t = &t;
  ctx.p = p;
  XML_SetHashSalt16Bytes(p, ORACLE_HASH_SALT);
  XML_SetUserData(p, &ctx);
  XML_SetParamEntityParsing(
      p, (enum XML_ParamEntityParsing)opts->param_entity_parsing);
  XML_SetXmlDeclHandler(p, on_xmldecl_w);
  XML_SetDoctypeDeclHandler(p, on_doctype_w, on_enddoctype_w);
  XML_SetElementHandler(p, on_start, on_end);
  XML_SetCharacterDataHandler(p, on_text);
  XML_SetProcessingInstructionHandler(p, on_pi);
  XML_SetCommentHandler(p, on_comment);
  XML_SetCdataSectionHandler(p, on_cdata, on_endcdata);
  XML_SetEntityDeclHandler(p, on_entity);
  XML_SetNotationDeclHandler(p, on_notation);
  XML_SetElementDeclHandler(p, on_element);
  XML_SetAttlistDeclHandler(p, on_attlist);
  XML_SetNamespaceDeclHandler(p, on_nsstart, on_nsend);
  XML_SetSkippedEntityHandler(p, on_skipped);
  if (opts->no_reparse_deferral)
    XML_SetReparseDeferralEnabled(p, XML_FALSE);

  if (opts->chunk_size == 0 || opts->chunk_size >= len) {
    st = feed(p, doc, len, 1, opts->buffer_api);
  } else {
    size_t off = 0;
    while (off < len && st == XML_STATUS_OK) {
      size_t n = len - off < opts->chunk_size ? len - off : opts->chunk_size;
      st = feed(p, doc + off, n, 0, opts->buffer_api);
      off += n;
    }
    if (st == XML_STATUS_OK)
      st = feed(p, (const unsigned char *)"", 0, 1, opts->buffer_api);
  }

  flush_text(&t);
  buf_str(&t.out, "result");
  buf_str(&t.out, st == XML_STATUS_OK ? " ok" : " error");
  buf_int(&t.out, (long long)XML_GetErrorCode(p));
  buf_uint(&t.out, (unsigned long long)XML_GetCurrentLineNumber64(p));
  buf_uint(&t.out, (unsigned long long)XML_GetCurrentColumnNumber64(p));
  buf_int(&t.out, (long long)XML_GetCurrentByteIndex64(p));
  buf_push(&t.out, "\n", 1);

  XML_ParserFree(p);
  free(t.text.data);
  if (t.out.failed) {
    free(t.out.data);
    return -1;
  }
  *out = t.out.data;
  *out_len = t.out.len;
  return 0;
}

void
oracle_free(void *p) {
  free(p);
}

const char *
oracle_expat_version(void) {
  return XML_ExpatVersion();
}
