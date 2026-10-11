/* Build shim (not part of upstream): white-box access to the string pool and
 * hash table of xmlparse.c (module 9).
 *
 * STRING_POOL, poolGrow, lookup, ... are `static` in xmlparse.c, so this
 * translation unit textually includes the unmodified vendored xmlparse.c and
 * runs small programs of pool / hash table operations against it, writing a
 * text trace that tests/src/core.rs reproduces with the Rust port. The
 * program and trace formats are documented there.
 *
 * Every symbol xmlparse.c exports is renamed with an `oracle_p_` prefix so
 * this second copy does not collide with the copy linked into the full
 * parser (a missing rename is a duplicate-symbol link error). Nothing in
 * vendor/ is modified. See docs/DECISIONS.md (D-009, D-021).
 */
#define XML_DefaultCurrent oracle_p_XML_DefaultCurrent
#define XML_ErrorString oracle_p_XML_ErrorString
#define XML_ExpatVersion oracle_p_XML_ExpatVersion
#define XML_ExpatVersionInfo oracle_p_XML_ExpatVersionInfo
#define XML_ExternalEntityParserCreate oracle_p_XML_ExternalEntityParserCreate
#define XML_FreeContentModel oracle_p_XML_FreeContentModel
#define XML_GetBase oracle_p_XML_GetBase
#define XML_GetBuffer oracle_p_XML_GetBuffer
#define XML_GetCurrentByteCount oracle_p_XML_GetCurrentByteCount
#define XML_GetCurrentByteCount64 oracle_p_XML_GetCurrentByteCount64
#define XML_GetCurrentByteIndex oracle_p_XML_GetCurrentByteIndex
#define XML_GetCurrentByteIndex64 oracle_p_XML_GetCurrentByteIndex64
#define XML_GetCurrentColumnNumber oracle_p_XML_GetCurrentColumnNumber
#define XML_GetCurrentColumnNumber64 oracle_p_XML_GetCurrentColumnNumber64
#define XML_GetCurrentLineNumber oracle_p_XML_GetCurrentLineNumber
#define XML_GetCurrentLineNumber64 oracle_p_XML_GetCurrentLineNumber64
#define XML_GetErrorCode oracle_p_XML_GetErrorCode
#define XML_GetFeatureList oracle_p_XML_GetFeatureList
#define XML_GetIdAttributeIndex oracle_p_XML_GetIdAttributeIndex
#define XML_GetInputContext oracle_p_XML_GetInputContext
#define XML_GetInputContext64 oracle_p_XML_GetInputContext64
#define XML_GetParsingStatus oracle_p_XML_GetParsingStatus
#define XML_GetPropertyBool oracle_p_XML_GetPropertyBool
#define XML_GetPropertyDouble oracle_p_XML_GetPropertyDouble
#define XML_GetPropertyUInt64 oracle_p_XML_GetPropertyUInt64
#define XML_GetSpecifiedAttributeCount oracle_p_XML_GetSpecifiedAttributeCount
#define XML_MemFree oracle_p_XML_MemFree
#define XML_MemMalloc oracle_p_XML_MemMalloc
#define XML_MemRealloc oracle_p_XML_MemRealloc
#define XML_Parse oracle_p_XML_Parse
#define XML_ParseBuffer oracle_p_XML_ParseBuffer
#define XML_ParserCreate oracle_p_XML_ParserCreate
#define XML_ParserCreateNS oracle_p_XML_ParserCreateNS
#define XML_ParserCreate_MM oracle_p_XML_ParserCreate_MM
#define XML_ParserFree oracle_p_XML_ParserFree
#define XML_ParserReset oracle_p_XML_ParserReset
#define XML_ResumeParser oracle_p_XML_ResumeParser
#define XML_SetAllocTrackerActivationThreshold oracle_p_XML_SetAllocTrackerActivationThreshold
#define XML_SetAllocTrackerMaximumAmplification oracle_p_XML_SetAllocTrackerMaximumAmplification
#define XML_SetAttlistDeclHandler oracle_p_XML_SetAttlistDeclHandler
#define XML_SetBase oracle_p_XML_SetBase
#define XML_SetBillionLaughsAttackProtectionActivationThreshold oracle_p_XML_SetBillionLaughsAttackProtectionActivationThreshold
#define XML_SetBillionLaughsAttackProtectionMaximumAmplification oracle_p_XML_SetBillionLaughsAttackProtectionMaximumAmplification
#define XML_SetCdataSectionHandler oracle_p_XML_SetCdataSectionHandler
#define XML_SetCharacterDataHandler oracle_p_XML_SetCharacterDataHandler
#define XML_SetCommentHandler oracle_p_XML_SetCommentHandler
#define XML_SetDefaultHandler oracle_p_XML_SetDefaultHandler
#define XML_SetDefaultHandlerExpand oracle_p_XML_SetDefaultHandlerExpand
#define XML_SetDoctypeDeclHandler oracle_p_XML_SetDoctypeDeclHandler
#define XML_SetElementDeclHandler oracle_p_XML_SetElementDeclHandler
#define XML_SetElementHandler oracle_p_XML_SetElementHandler
#define XML_SetEncoding oracle_p_XML_SetEncoding
#define XML_SetEndCdataSectionHandler oracle_p_XML_SetEndCdataSectionHandler
#define XML_SetEndDoctypeDeclHandler oracle_p_XML_SetEndDoctypeDeclHandler
#define XML_SetEndElementHandler oracle_p_XML_SetEndElementHandler
#define XML_SetEndNamespaceDeclHandler oracle_p_XML_SetEndNamespaceDeclHandler
#define XML_SetEntityDeclHandler oracle_p_XML_SetEntityDeclHandler
#define XML_SetExternalEntityRefHandler oracle_p_XML_SetExternalEntityRefHandler
#define XML_SetExternalEntityRefHandlerArg oracle_p_XML_SetExternalEntityRefHandlerArg
#define XML_SetHashSalt oracle_p_XML_SetHashSalt
#define XML_SetHashSalt16Bytes oracle_p_XML_SetHashSalt16Bytes
#define XML_SetNamespaceDeclHandler oracle_p_XML_SetNamespaceDeclHandler
#define XML_SetNotStandaloneHandler oracle_p_XML_SetNotStandaloneHandler
#define XML_SetNotationDeclHandler oracle_p_XML_SetNotationDeclHandler
#define XML_SetParamEntityParsing oracle_p_XML_SetParamEntityParsing
#define XML_SetProcessingInstructionHandler oracle_p_XML_SetProcessingInstructionHandler
#define XML_SetPropertyBool oracle_p_XML_SetPropertyBool
#define XML_SetPropertyDouble oracle_p_XML_SetPropertyDouble
#define XML_SetPropertyUInt64 oracle_p_XML_SetPropertyUInt64
#define XML_SetReparseDeferralEnabled oracle_p_XML_SetReparseDeferralEnabled
#define XML_SetReturnNSTriplet oracle_p_XML_SetReturnNSTriplet
#define XML_SetSkippedEntityHandler oracle_p_XML_SetSkippedEntityHandler
#define XML_SetStartCdataSectionHandler oracle_p_XML_SetStartCdataSectionHandler
#define XML_SetStartDoctypeDeclHandler oracle_p_XML_SetStartDoctypeDeclHandler
#define XML_SetStartElementHandler oracle_p_XML_SetStartElementHandler
#define XML_SetStartNamespaceDeclHandler oracle_p_XML_SetStartNamespaceDeclHandler
#define XML_SetUnknownEncodingHandler oracle_p_XML_SetUnknownEncodingHandler
#define XML_SetUnparsedEntityDeclHandler oracle_p_XML_SetUnparsedEntityDeclHandler
#define XML_SetUserData oracle_p_XML_SetUserData
#define XML_SetXmlDeclHandler oracle_p_XML_SetXmlDeclHandler
#define XML_StopParser oracle_p_XML_StopParser
#define XML_UseForeignDTD oracle_p_XML_UseForeignDTD
#define XML_UseParserAsHandlerArg oracle_p_XML_UseParserAsHandlerArg
#define g_reparseDeferralEnabledDefault oracle_p_g_reparseDeferralEnabledDefault
#define testingAccountingGetCountBytesDirect oracle_p_testingAccountingGetCountBytesDirect
#define testingAccountingGetCountBytesIndirect oracle_p_testingAccountingGetCountBytesIndirect
#define unsignedCharToPrintable oracle_p_unsignedCharToPrintable

#include "xmlparse.c"

#include <stdarg.h>

#include "oracle.h"

/* ---- allocation hooks -------------------------------------------------- */

/* The rule both sides implement (tests/src/core.rs `HarnessHeap`): attempt
 * number `fail_at` fails, and so does any request that would take the total
 * above ORACLE_CORE_HEAP_CAP bytes. The allocation tracker is disabled, so
 * these are the only refusals. Callers serialise runs (the state is
 * global). */
#define HDR (sizeof(size_t) + EXPAT_MALLOC_PADDING)

static struct {
  int recording;
  unsigned long long attempts;
  unsigned long long fail_at; /* ULLONG_MAX: never */
  size_t total;
} g_heap;

struct obuf {
  char *data;
  size_t len, cap;
  int failed;
};

static void
ob_put(struct obuf *b, const char *s, size_t n) {
  if (b->failed)
    return;
  if (b->len + n > b->cap) {
    size_t cap = b->cap ? b->cap * 2 : 4096;
    while (cap < b->len + n)
      cap *= 2;
    char *d = realloc(b->data, cap);
    if (! d) {
      b->failed = 1;
      return;
    }
    b->data = d;
    b->cap = cap;
  }
  memcpy(b->data + b->len, s, n);
  b->len += n;
}

static void
ob_printf(struct obuf *b, const char *fmt, ...) {
  char tmp[256];
  va_list ap;
  va_start(ap, fmt);
  int n = vsnprintf(tmp, sizeof tmp, fmt, ap);
  va_end(ap);
  if (n < 0 || (size_t)n >= sizeof tmp) {
    b->failed = 1;
    return;
  }
  ob_put(b, tmp, (size_t)n);
}

static void
ob_hex(struct obuf *b, const unsigned char *p, size_t n) {
  static const char digits[] = "0123456789abcdef";
  for (size_t i = 0; i < n; i++) {
    char two[2] = {digits[p[i] >> 4], digits[p[i] & 15]};
    ob_put(b, two, 2);
  }
}

static struct obuf *g_out;

static int
heap_decide(size_t increase) {
  int ok = g_heap.attempts != g_heap.fail_at
           && increase <= ORACLE_CORE_HEAP_CAP - g_heap.total;
  g_heap.attempts++;
  return ok;
}

static void *
hook_malloc(size_t n) {
  if (! g_heap.recording)
    return malloc(n);
  size_t size = n - HDR;
  int ok = heap_decide(size);
  ob_printf(g_out, "  alloc %zu %s\n", size, ok ? "ok" : "fail");
  if (! ok)
    return NULL;
  void *p = malloc(n);
  if (! p)
    abort(); /* the harness's own allocation failed: not a test outcome */
  g_heap.total += size;
  return p;
}

static void *
hook_realloc(void *p, size_t n) {
  if (! g_heap.recording)
    return realloc(p, n);
  size_t old = *(size_t *)p;
  size_t size = n - HDR;
  int ok = heap_decide(size > old ? size - old : 0);
  ob_printf(g_out, "  realloc %zu %zu %s\n", old, size, ok ? "ok" : "fail");
  if (! ok)
    return NULL;
  void *q = realloc(p, n);
  if (! q)
    abort();
  g_heap.total = g_heap.total - old + size;
  return q;
}

static void
hook_free(void *p) {
  if (g_heap.recording && p) {
    size_t size = *(size_t *)p;
    ob_printf(g_out, "  free %zu\n", size);
    g_heap.total -= size;
  }
  free(p);
}

/* ---- program reader ---------------------------------------------------- */

struct reader {
  const unsigned char *p, *end;
  int bad;
};

static unsigned
rd_u8(struct reader *r) {
  if (r->p >= r->end) {
    r->bad = 1;
    return 0;
  }
  return *r->p++;
}

static unsigned long
rd_u32(struct reader *r) {
  unsigned long v = 0;
  for (int i = 0; i < 4; i++)
    v |= (unsigned long)rd_u8(r) << (8 * i);
  return v;
}

/* A length-prefixed string; returns a malloc'd NUL-terminated copy. */
static char *
rd_str(struct reader *r, size_t *len) {
  size_t n = rd_u32(r);
  if (r->bad || (size_t)(r->end - r->p) < n) {
    r->bad = 1;
    return NULL;
  }
  char *s = malloc(n + 1);
  if (! s)
    abort();
  memcpy(s, r->p, n);
  s[n] = 0;
  r->p += n;
  *len = n;
  return s;
}

static unsigned long long
fnv(unsigned long long h, const void *data, size_t n) {
  const unsigned char *p = data;
  for (size_t i = 0; i < n; i++) {
    h ^= p[i];
    h *= 0x100000001b3ULL;
  }
  return h;
}
#define FNV_INIT 0xcbf29ce484222325ULL

static XML_Parser
new_parser(const unsigned char salt[16]) {
  static const XML_Memory_Handling_Suite mem
      = {hook_malloc, hook_realloc, hook_free};
  XML_Parser p = XML_ParserCreate_MM(NULL, &mem, NULL);
  if (! p)
    abort();
  XML_SetHashSalt16Bytes(p, salt);
  XML_SetAllocTrackerActivationThreshold(p, (unsigned long long)-1);
  return p;
}

/* ---- string pool ------------------------------------------------------- */

static int
block_count(const BLOCK *b) {
  int n = 0;
  for (; b; b = b->next)
    n++;
  return n;
}

/* Sizes oldest first (list tail first). */
static void
print_sizes(struct obuf *o, const BLOCK *b) {
  int n = block_count(b);
  int *v = malloc(sizeof(int) * (n ? n : 1));
  if (! v)
    abort();
  for (int i = n - 1; b; b = b->next, i--)
    v[i] = b->size;
  ob_put(o, "[", 1);
  for (int i = 0; i < n; i++)
    ob_printf(o, i ? ",%d" : "%d", v[i]);
  ob_put(o, "]", 1);
  free(v);
}

/* Block index (oldest = 0) and offset of `p`, searching from the newest. */
static void
locate(const STRING_POOL *pool, const XML_Char *p, int *block, long *off) {
  int n = block_count(pool->blocks);
  int i = n - 1;
  for (const BLOCK *b = pool->blocks; b; b = b->next, i--) {
    if (p >= b->s && p <= b->s + b->size) {
      *block = i;
      *off = (long)(p - b->s);
      return;
    }
  }
  *block = -1;
  *off = -1;
}

struct region {
  const XML_Char *p;
  size_t len;
};

static const ENCODING *
enc_by_id(unsigned id) {
  const void *e = oracle_encoding((int)(id % 12));
  if (! e)
    abort();
  return e;
}

static void
run_pool(struct reader *r, XML_Parser parser, struct obuf *o) {
  STRING_POOL pool;
  poolInit(&pool, parser);
  struct region *regions = NULL;
  size_t nregions = 0, capregions = 0;
  g_heap.recording = 1;
  for (int idx = 0; r->p < r->end && ! r->bad; idx++) {
    unsigned op = rd_u8(r);
    size_t len = 0;
    char *s = NULL;
    const XML_Char *ret = NULL;
    int has_ret = 0, ok = 1;
    int finished = 0;
    switch (op) {
    case 0: {
      unsigned c = rd_u8(r);
      ob_printf(o, "%d append_char %02x\n", idx, c);
      ok = poolAppendChar(&pool, (XML_Char)c);
      break;
    }
    case 1:
      s = rd_str(r, &len);
      if (r->bad)
        break;
      ob_printf(o, "%d append_chars %zu\n", idx, len);
      ok = poolAppendChars(&pool, s, len);
      break;
    case 2:
    case 3: {
      unsigned e = rd_u8(r);
      s = rd_str(r, &len);
      if (r->bad)
        break;
      ob_printf(o, "%d %s %u %zu\n", idx, op == 2 ? "append" : "store_string",
                e % 12, len);
      const ENCODING *enc = enc_by_id(e);
      ret = op == 2 ? poolAppend(&pool, enc, s, s + len)
                    : poolStoreString(&pool, enc, s, s + len);
      has_ret = 1;
      break;
    }
    case 4:
      s = rd_str(r, &len);
      if (r->bad)
        break;
      ob_printf(o, "%d copy_string %zu\n", idx, len);
      ret = poolCopyString(&pool, s);
      has_ret = 1;
      finished = ret != NULL;
      break;
    case 5:
      s = rd_str(r, &len);
      if (r->bad)
        break;
      ob_printf(o, "%d copy_string_no_finish %zu\n", idx, len);
      ret = poolCopyStringNoFinish(&pool, s);
      has_ret = 1;
      break;
    case 6:
      s = rd_str(r, &len);
      if (r->bad)
        break;
      ob_printf(o, "%d copy_string_n %zu\n", idx, len);
      ret = poolCopyStringN(&pool, s, (int)len);
      has_ret = 1;
      finished = ret != NULL;
      break;
    case 7:
      s = rd_str(r, &len);
      if (r->bad)
        break;
      ob_printf(o, "%d append_string %zu\n", idx, len);
      ret = poolAppendString(&pool, s);
      has_ret = 1;
      break;
    case 8:
      ob_printf(o, "%d grow\n", idx);
      ok = poolGrow(&pool);
      break;
    case 9: {
      unsigned long n = rd_u32(r);
      ob_printf(o, "%d grow_until %lu\n", idx, n);
      ok = poolGrowUntil(&pool, n);
      break;
    }
    case 10:
      ob_printf(o, "%d finish\n", idx);
      finished = 1;
      break;
    case 11:
      ob_printf(o, "%d discard\n", idx);
      poolDiscard(&pool);
      break;
    case 12:
      ob_printf(o, "%d chop\n", idx);
      if (poolLength(&pool) > 0) {
        STRING_POOL *pp = &pool; /* the macro does not parenthesise */
        poolChop(pp);
      }
      break;
    case 13:
      ob_printf(o, "%d clear\n", idx);
      poolClear(&pool);
      nregions = 0;
      break;
    case 14: {
      unsigned long k = rd_u32(r);
      ob_printf(o, "%d fail_next %lu\n", idx, k);
      g_heap.fail_at = g_heap.attempts + k;
      break;
    }
    case 15:
      if (poolLength(&pool) > 0)
        ob_printf(o, "%d last_char %02x\n", idx,
                  (unsigned char)poolLastChar(&pool));
      else
        ob_printf(o, "%d last_char none\n", idx);
      break;
    default:
      r->bad = 1;
      break;
    }
    if (r->bad) {
      free(s);
      ob_printf(o, "end of program\n");
      break;
    }
    /* Record a finished string (the current string before poolFinish). */
    if (finished) {
      const XML_Char *from = op == 10 ? pool.start : ret;
      if (op == 10)
        poolFinish(&pool);
      size_t n = (size_t)(pool.start - from);
      if (from && n > 0) {
        if (nregions == capregions) {
          capregions = capregions ? capregions * 2 : 16;
          regions = realloc(regions, capregions * sizeof *regions);
          if (! regions)
            abort();
        }
        regions[nregions].p = from;
        regions[nregions].len = n;
        nregions++;
      }
    }
    if (has_ret) {
      if (ret) {
        int b;
        long off;
        locate(&pool, ret, &b, &off);
        ob_printf(o, "  -> str %d %ld\n", b, off);
      } else {
        ob_printf(o, "  -> fail\n");
      }
    } else {
      ob_printf(o, "  -> %s\n", ok ? "ok" : "fail");
    }
    ob_put(o, "  blocks ", 9);
    print_sizes(o, pool.blocks);
    ob_put(o, " free ", 6);
    print_sizes(o, pool.freeBlocks);
    if (pool.start) {
      ob_printf(o, " cur %ld %ld ", (long)(pool.start - pool.blocks->s),
                (long)(pool.ptr - pool.blocks->s));
      size_t n = (size_t)(pool.ptr - pool.start);
      if (n <= 64)
        ob_hex(o, (const unsigned char *)pool.start, n);
      else
        ob_printf(o, "#%zu:%016llx", n, fnv(FNV_INIT, pool.start, n));
    } else {
      ob_printf(o, " cur null");
    }
    unsigned long long h = FNV_INIT;
    for (size_t i = 0; i < nregions; i++) {
      h = fnv(h, &regions[i].len, sizeof(size_t));
      h = fnv(h, regions[i].p, regions[i].len);
    }
    ob_printf(o, " regions %zu %016llx\n", nregions, h);
    free(s);
  }
  ob_printf(o, "destroy\n");
  poolDestroy(&pool);
  g_heap.recording = 0;
  free(regions);
}

/* ---- hash table -------------------------------------------------------- */

typedef struct {
  NAMED named;
  size_t id;
} ENTRY;

/* The slot holding `e`: follows lookup's own probe sequence (hash() and
 * PROBE_STEP of xmlparse.c) until it meets `e`; -1 if it meets an empty slot
 * first, which would mean the entry is unreachable. */
static long
slot_of(XML_Parser parser, const HASH_TABLE *t, const NAMED *e) {
  if (t->size == 0)
    return -1;
  unsigned long h = hash(parser, e->name, keylen(e->name));
  unsigned long mask = (unsigned long)t->size - 1;
  unsigned char step = 0;
  size_t i = h & mask;
  while (t->v[i]) {
    if (t->v[i] == e)
      return (long)i;
    if (! step)
      step = PROBE_STEP(h, mask, t->power);
    i < step ? (i += t->size - step) : (i -= step);
  }
  return -1;
}

static void
run_hash(struct reader *r, XML_Parser parser, struct obuf *o) {
  HASH_TABLE table;
  hashTableInit(&table, parser);
  char **keys = NULL;
  size_t nkeys = 0, capkeys = 0;
  size_t next_id = 0;
  g_heap.recording = 1;
  for (int idx = 0; r->p < r->end && ! r->bad; idx++) {
    unsigned op = rd_u8(r);
    size_t len = 0;
    char *s = NULL;
    switch (op) {
    case 0: {
      s = rd_str(r, &len);
      unsigned long create = rd_u32(r);
      if (r->bad)
        break;
      if (create < sizeof(ENTRY))
        create = sizeof(ENTRY);
      ob_printf(o, "%d insert %zu ", idx, len);
      ob_hex(o, (const unsigned char *)s, len);
      ob_printf(o, " %lu\n", create);
      /* The key must outlive the table: keep it. */
      if (nkeys == capkeys) {
        capkeys = capkeys ? capkeys * 2 : 16;
        keys = realloc(keys, capkeys * sizeof *keys);
        if (! keys)
          abort();
      }
      keys[nkeys++] = s;
      ENTRY *e = (ENTRY *)lookup(parser, &table, s, create);
      s = NULL;
      if (! e) {
        ob_printf(o, "  -> fail\n");
      } else if (e->id == 0 && e->named.name == keys[nkeys - 1]) {
        /* Freshly zeroed by lookup: assign the next id (ids are stored
         * plus one so that 0 means "new"). */
        e->id = ++next_id;
        ob_printf(o, "  -> inserted %zu %ld\n", e->id - 1,
                  slot_of(parser, &table, &e->named));
      } else {
        ob_printf(o, "  -> found %zu %ld\n", e->id - 1,
                  slot_of(parser, &table, &e->named));
      }
      break;
    }
    case 1: {
      s = rd_str(r, &len);
      if (r->bad)
        break;
      ob_printf(o, "%d find %zu ", idx, len);
      ob_hex(o, (const unsigned char *)s, len);
      ob_put(o, "\n", 1);
      ENTRY *e = (ENTRY *)lookupWithLength(parser, &table, s, len, 0);
      if (e)
        ob_printf(o, "  -> found %zu %ld\n", e->id - 1,
                  slot_of(parser, &table, &e->named));
      else
        ob_printf(o, "  -> none\n");
      break;
    }
    case 2:
      ob_printf(o, "%d clear\n", idx);
      hashTableClear(&table);
      next_id = 0;
      break;
    case 3: {
      ob_printf(o, "%d iterate\n", idx);
      HASH_TABLE_ITER it;
      hashTableIterInit(&it, &table);
      ob_put(o, "  ->", 4);
      for (NAMED *e; (e = hashTableIterNext(&it)) != NULL;)
        ob_printf(o, " %zu", ((ENTRY *)e)->id - 1);
      ob_put(o, "\n", 1);
      break;
    }
    case 4: {
      unsigned long k = rd_u32(r);
      ob_printf(o, "%d fail_next %lu\n", idx, k);
      g_heap.fail_at = g_heap.attempts + k;
      break;
    }
    default:
      r->bad = 1;
      break;
    }
    free(s);
    if (r->bad) {
      ob_printf(o, "end of program\n");
      break;
    }
    ob_printf(o, "  power %u size %zu used %zu\n", (unsigned)table.power,
              table.size, table.used);
  }
  ob_put(o, "slots", 5);
  for (size_t i = 0; i < table.size; i++) {
    if (table.v[i])
      ob_printf(o, " %zu", ((ENTRY *)table.v[i])->id - 1);
    else
      ob_put(o, " -", 2);
  }
  ob_printf(o, "\ndestroy\n");
  hashTableDestroy(&table);
  g_heap.recording = 0;
  for (size_t i = 0; i < nkeys; i++)
    free(keys[i]);
  free(keys);
}

/* ---- entry points ------------------------------------------------------ */

int
oracle_core_run(const unsigned char *prog, size_t len, unsigned char **out,
                size_t *out_len) {
  struct reader r = {prog, prog + len, 0};
  unsigned kind = rd_u8(&r);
  unsigned char salt[16];
  for (int i = 0; i < 16; i++)
    salt[i] = (unsigned char)rd_u8(&r);
  struct obuf o = {NULL, 0, 0, 0};
  if (r.bad || kind > 1)
    return -1;
  g_out = &o;
  g_heap.recording = 0;
  g_heap.attempts = 0;
  g_heap.fail_at = (unsigned long long)-1;
  g_heap.total = 0;
  XML_Parser parser = new_parser(salt);
  if (kind == 0)
    run_pool(&r, parser, &o);
  else
    run_hash(&r, parser, &o);
  XML_ParserFree(parser);
  g_out = NULL;
  if (o.failed) {
    free(o.data);
    return -1;
  }
  *out = (unsigned char *)o.data;
  *out_len = o.len;
  return 0;
}

size_t
oracle_pool_bytes_to_allocate_for(int block_size) {
  return poolBytesToAllocateFor(block_size);
}

size_t
oracle_block_header_size(void) {
  return offsetof(BLOCK, s);
}

unsigned long long
oracle_parser_hash(const unsigned char salt[16], const char *key, size_t len,
                   int *ulong_bits) {
  XML_Parser p = new_parser(salt);
  unsigned long h = hash(p, key, len);
  XML_ParserFree(p);
  *ulong_bits = (int)(sizeof(unsigned long) * CHAR_BIT);
  return (unsigned long long)h;
}
