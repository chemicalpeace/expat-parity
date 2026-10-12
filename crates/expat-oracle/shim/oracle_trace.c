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

#include "expat_config.h" /* XML_GE: declares the limit setters */
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
  int digest;              /* text_digest option */
  unsigned long long dlen; /* digest mode: length of the pending text */
  uint64_t dhash;          /* digest mode: FNV-1a 64 of the pending text */
};

/* From internal.h (exported by xmlparse.c for its own test suite). */
unsigned long long testingAccountingGetCountBytesDirect(XML_Parser parser);
unsigned long long testingAccountingGetCountBytesIndirect(XML_Parser parser);

#define FNV_OFFSET 0xcbf29ce484222325ULL
#define FNV_PRIME 0x100000001b3ULL

/* Capped allocator of the heap_cap option. The parse runs on one thread,
   so thread-local counters keep concurrent tests apart. Every block
   carries its size in a header. */
#if defined(_MSC_VER)
#  define ORACLE_TLS __declspec(thread)
#else
#  define ORACLE_TLS _Thread_local
#endif
static ORACLE_TLS size_t heap_cap_bytes;
static ORACLE_TLS size_t heap_live;
static ORACLE_TLS size_t heap_peak;

size_t
oracle_last_heap_peak(void) {
  return heap_peak;
}

/* Allocation tracker readouts of the calling thread's last parse
   (module 11c): the tracker's live byte count after XML_ParserCreate and
   the limit setters, and at the end of the parse (before
   XML_ParserFree). */
unsigned long long oracle_tracker_bytes(const void *parser); /* oracle_core.c */
static ORACLE_TLS unsigned long long tracker_created;
static ORACLE_TLS unsigned long long tracker_end;

void
oracle_last_tracker(unsigned long long *created, unsigned long long *end) {
  *created = tracker_created;
  *end = tracker_end;
}
#define HEAP_HDR 16

static void *
capped_malloc(size_t n) {
  if (n > heap_cap_bytes || heap_live > heap_cap_bytes - n)
    return NULL;
  unsigned char *b = malloc(n + HEAP_HDR);
  if (b == NULL)
    return NULL;
  memcpy(b, &n, sizeof n);
  heap_live += n;
  if (heap_live > heap_peak)
    heap_peak = heap_live;
  return b + HEAP_HDR;
}

static void
capped_free(void *p) {
  if (p == NULL)
    return;
  unsigned char *b = (unsigned char *)p - HEAP_HDR;
  size_t n;
  memcpy(&n, b, sizeof n);
  heap_live -= n;
  free(b);
}

static void *
capped_realloc(void *p, size_t n) {
  if (p == NULL)
    return capped_malloc(n);
  unsigned char *b = (unsigned char *)p - HEAP_HDR;
  size_t old;
  memcpy(&old, b, sizeof old);
  if (n > old && (n - old > heap_cap_bytes || heap_live > heap_cap_bytes - (n - old)))
    return NULL;
  unsigned char *nb = realloc(b, n + HEAP_HDR);
  if (nb == NULL)
    return NULL;
  memcpy(nb, &n, sizeof n);
  heap_live = heap_live - old + n;
  if (heap_live > heap_peak)
    heap_peak = heap_live;
  return nb + HEAP_HDR;
}

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
  if (t->digest) {
    char tmp[64];
    int n = snprintf(tmp, sizeof tmp, "textdigest %llu %016llx\n", t->dlen,
                     (unsigned long long)t->dhash);
    if (n > 0)
      buf_push(&t->out, tmp, (size_t)n);
    t->dlen = 0;
    t->dhash = FNV_OFFSET;
    t->in_text = 0;
    return;
  }
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
  if (t->digest) {
    for (int i = 0; i < len; i++) {
      t->dhash ^= (unsigned char)s[i];
      t->dhash *= FNV_PRIME;
    }
    t->dlen += (unsigned long long)(len > 0 ? len : 0);
    return;
  }
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

/* Writes the model as `<type><quant> <name>(<child>,<child>...)` without
   recursion (an explicit heap stack), so a deeply nested model cannot
   overflow the C stack of a test thread (2 MiB on macOS/Windows test
   threads; a 20,000-deep model did with the earlier recursive version). */
struct model_frame {
  const XML_Content *node;
  unsigned int next;
};

static void
model(struct buf *b, const XML_Content *m) {
  size_t cap = 64, len = 0;
  struct model_frame *st = malloc(cap * sizeof *st);
  if (st == NULL) {
    b->failed = 1;
    return;
  }
  for (;;) {
    char head[8];
    snprintf(head, sizeof head, "%d%d", (int)m->type, (int)m->quant);
    buf_str(b, head);
    buf_field(b, m->name);
    buf_push(b, "(", 1);
    if (len == cap) {
      struct model_frame *grown;
      cap *= 2;
      grown = realloc(st, cap * sizeof *st);
      if (grown == NULL) {
        b->failed = 1;
        free(st);
        return;
      }
      st = grown;
    }
    st[len].node = m;
    st[len].next = 0;
    len++;
    /* Close finished nodes; descend into the next unvisited child. */
    m = NULL;
    while (len > 0) {
      struct model_frame *f = &st[len - 1];
      if (f->next < f->node->numchildren) {
        if (f->next > 0)
          buf_push(b, ",", 1);
        m = &f->node->children[f->next];
        f->next++;
        break;
      }
      buf_push(b, ")", 1);
      len--;
    }
    if (m == NULL)
      break;
  }
  free(st);
}

static void XMLCALL
on_element(void *ud, const XML_Char *name, XML_Content *m) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "element");
  buf_field(&t->out, name);
  buf_push(&t->out, " ", 1);
  model(&t->out, m);
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

/* Module 11d test handlers (oracle_options.ext_handler / not_standalone).
   The external entity reference handler's argument is the parser
   (libexpat's default), whose user data is the start context. */
static ORACLE_TLS int ext_handler_mode;
static ORACLE_TLS int not_standalone_mode;
static ORACLE_TLS const unsigned char *ext_data;
static ORACLE_TLS size_t ext_len;
static ORACLE_TLS size_t ext_chunk;
static ORACLE_TLS int ext_buffer_api;
static ORACLE_TLS int ext_depth;

static enum XML_Status feed(XML_Parser p, const unsigned char *s, size_t n,
                            int final, int buffer_api);

static unsigned long
rd32(const unsigned char *b) {
  return (unsigned long)b[0] | ((unsigned long)b[1] << 8)
         | ((unsigned long)b[2] << 16) | ((unsigned long)b[3] << 24);
}

/* The in-memory entity of system identifier `id`. */
static int
ext_lookup(const XML_Char *id, const unsigned char **body, size_t *blen) {
  size_t off = 0, idlen = strlen(id);
  while (off + 4 <= ext_len) {
    size_t klen = rd32(ext_data + off);
    const unsigned char *key = ext_data + off + 4;
    off += 4 + klen;
    if (off + 4 > ext_len)
      return 0;
    size_t vlen = rd32(ext_data + off);
    const unsigned char *val = ext_data + off + 4;
    off += 4 + vlen;
    if (off > ext_len)
      return 0;
    if (klen == idlen && memcmp(key, id, klen) == 0) {
      *body = val;
      *blen = vlen;
      return 1;
    }
  }
  return 0;
}

/* Reads one external entity from memory through an external entity
   parser (ext_handler 3). */
static int
read_external(XML_Parser p, struct tracer *t, const XML_Char *context,
              const unsigned char *body, size_t len) {
  if (ext_depth >= ORACLE_EXT_MAX_DEPTH) {
    end_line(begin(t, "extdepth"));
    return XML_STATUS_ERROR;
  }
  XML_Parser child = XML_ExternalEntityParserCreate(p, context, NULL);
  if (child == NULL) {
    end_line(begin(t, "extcreate"));
    return XML_STATUS_ERROR;
  }
  struct start_ctx cc;
  cc.t = t;
  cc.p = child;
  XML_SetUserData(child, &cc);
  ext_depth++;
  enum XML_Status st = XML_STATUS_OK;
  if (ext_chunk == 0 || ext_chunk >= len) {
    st = feed(child, body, len, 1, ext_buffer_api);
  } else {
    size_t off = 0;
    while (off < len && st == XML_STATUS_OK) {
      size_t n = len - off < ext_chunk ? len - off : ext_chunk;
      st = feed(child, body + off, n, 0, ext_buffer_api);
      off += n;
    }
    if (st == XML_STATUS_OK)
      st = feed(child, (const unsigned char *)"", 0, 1, ext_buffer_api);
  }
  ext_depth--;
  begin(t, "extend");
  buf_int(&t->out, st == XML_STATUS_OK);
  buf_int(&t->out, (long long)XML_GetErrorCode(child));
  buf_uint(&t->out, (unsigned long long)XML_GetCurrentLineNumber64(child));
  buf_uint(&t->out, (unsigned long long)XML_GetCurrentColumnNumber64(child));
  buf_int(&t->out, (long long)XML_GetCurrentByteIndex64(child));
  end_line(t);
  XML_ParserFree(child);
  return st == XML_STATUS_OK ? XML_STATUS_OK : XML_STATUS_ERROR;
}

static int XMLCALL
on_extref(XML_Parser p, const XML_Char *context, const XML_Char *base,
          const XML_Char *system_id, const XML_Char *public_id) {
  struct start_ctx *c = XML_GetUserData(p);
  struct tracer *t = begin(c->t, "extref");
  buf_field(&t->out, context);
  buf_field(&t->out, base);
  buf_field(&t->out, system_id);
  buf_field(&t->out, public_id);
  end_line(t);
  if (ext_handler_mode == 3) {
    const unsigned char *body;
    size_t blen;
    if (system_id == NULL || ! ext_lookup(system_id, &body, &blen))
      return XML_STATUS_OK;
    return read_external(p, c->t, context, body, blen);
  }
  return ext_handler_mode == 2 ? XML_STATUS_OK : XML_STATUS_ERROR;
}

static int XMLCALL
on_not_standalone(void *ud) {
  struct start_ctx *c = ud;
  struct tracer *t = begin(c->t, "notstandalone");
  end_line(t);
  return not_standalone_mode == 1;
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

  t.digest = opts->text_digest;
  t.dhash = FNV_OFFSET;
  if (opts->heap_cap != 0) {
    static const XML_Memory_Handling_Suite capped
        = {capped_malloc, capped_realloc, capped_free};
    XML_Char sep = (XML_Char)opts->ns_separator;
    heap_cap_bytes = opts->heap_cap;
    heap_live = 0;
    heap_peak = 0;
    p = XML_ParserCreate_MM(opts->encoding, &capped,
                            opts->ns_separator != 0 ? &sep : NULL);
  } else if (opts->ns_separator != 0)
    p = XML_ParserCreateNS(opts->encoding, (XML_Char)opts->ns_separator);
  else
    p = XML_ParserCreate(opts->encoding);
  if (p == NULL)
    return -1;
  if (opts->set_bl_amplification) {
    buf_str(&t.out, "limit bl-amplification");
    buf_int(&t.out, XML_SetBillionLaughsAttackProtectionMaximumAmplification(
                        p, opts->bl_amplification));
    buf_push(&t.out, "\n", 1);
  }
  if (opts->set_bl_threshold) {
    buf_str(&t.out, "limit bl-threshold");
    buf_int(&t.out, XML_SetBillionLaughsAttackProtectionActivationThreshold(
                        p, opts->bl_threshold));
    buf_push(&t.out, "\n", 1);
  }
  if (opts->set_at_amplification) {
    buf_str(&t.out, "limit at-amplification");
    buf_int(&t.out,
            XML_SetAllocTrackerMaximumAmplification(p, opts->at_amplification));
    buf_push(&t.out, "\n", 1);
  }
  if (opts->set_at_threshold) {
    buf_str(&t.out, "limit at-threshold");
    buf_int(&t.out,
            XML_SetAllocTrackerActivationThreshold(p, opts->at_threshold));
    buf_push(&t.out, "\n", 1);
  }

  tracker_created = oracle_tracker_bytes(p);
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
  ext_handler_mode = opts->ext_handler;
  not_standalone_mode = opts->not_standalone;
  ext_data = opts->ext_data;
  ext_len = opts->ext_len;
  ext_chunk = opts->chunk_size;
  ext_buffer_api = opts->buffer_api;
  ext_depth = 0;
  if (opts->ext_handler)
    XML_SetExternalEntityRefHandler(p, on_extref);
  if (opts->not_standalone)
    XML_SetNotStandaloneHandler(p, on_not_standalone);
  if (opts->use_foreign_dtd)
    XML_UseForeignDTD(p, XML_TRUE);
  if (opts->no_reparse_deferral)
    XML_SetReparseDeferralEnabled(p, XML_FALSE);
  if (opts->ns_triplets)
    XML_SetReturnNSTriplet(p, XML_TRUE);

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
  if (opts->trace_accounting) {
    buf_str(&t.out, "accounting");
    buf_uint(&t.out, testingAccountingGetCountBytesDirect(p));
    buf_uint(&t.out, testingAccountingGetCountBytesIndirect(p));
    buf_push(&t.out, "\n", 1);
  }
  buf_str(&t.out, "result");
  buf_str(&t.out, st == XML_STATUS_OK ? " ok" : " error");
  buf_int(&t.out, (long long)XML_GetErrorCode(p));
  buf_uint(&t.out, (unsigned long long)XML_GetCurrentLineNumber64(p));
  buf_uint(&t.out, (unsigned long long)XML_GetCurrentColumnNumber64(p));
  buf_int(&t.out, (long long)XML_GetCurrentByteIndex64(p));
  buf_push(&t.out, "\n", 1);

  tracker_end = oracle_tracker_bytes(p);
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
