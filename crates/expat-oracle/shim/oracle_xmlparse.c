/* Build shim (not part of upstream): the translation unit of the full
 * parser's xmlparse.c, with the allocation tracker's event capture
 * (decision D-034).
 *
 * The vendored xmlparse.c is included unmodified. Two of its library calls
 * are routed through this shim, for this translation unit only:
 *
 * - getenv: during a capture, "EXPAT_MALLOC_DEBUG" reads as "2", which
 *   libexpat reads once in parserCreate (xmlparse.c 1364) and which makes
 *   expat_malloc / expat_realloc / expat_free report every tracked
 *   allocation through expat_heap_stat (851-856) and keep
 *   peakBytesAllocated. Every other name, and every call outside a capture,
 *   goes to the C library.
 * - fprintf: expat_heap_stat's report ("expat: Allocations(...)") becomes
 *   one event record; anything else is printed as before.
 *
 * Nothing else changes: the debug level only adds the reports, it does not
 * change what is allocated or refused. <stdio.h> and <stdlib.h> are included
 * before the macros, so their declarations keep the C library's names (on
 * MSVC, fprintf is an inline function in <stdio.h>). */
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "oracle.h"

int oracle_xp_fprintf(FILE *stream, const char *fmt, ...);
char *oracle_xp_getenv(const char *name);

#define fprintf oracle_xp_fprintf
#define getenv oracle_xp_getenv
#include "xmlparse.c"
#undef fprintf
#undef getenv

#if defined(_MSC_VER)
#  define ORACLE_XP_TLS __declspec(thread)
#else
#  define ORACLE_XP_TLS _Thread_local
#endif

static ORACLE_XP_TLS int capture_on;
static ORACLE_XP_TLS struct oracle_tracker_event *events;
static ORACLE_XP_TLS size_t events_len;
static ORACLE_XP_TLS size_t events_cap;
static ORACLE_XP_TLS int events_overflow;
static ORACLE_XP_TLS size_t events_at_end;

/* Upper bound of the events kept per capture; beyond it the capture is
   marked incomplete. */
#define ORACLE_EVENTS_MAX ((size_t)1 << 24)

char *
oracle_xp_getenv(const char *name) {
  static char two[] = "2";
  if (capture_on && strcmp(name, "EXPAT_MALLOC_DEBUG") == 0)
    return two;
  return getenv(name);
}

static void
push_event(const struct oracle_tracker_event *e) {
  if (events_len == events_cap) {
    size_t cap = events_cap ? events_cap * 2 : 1024;
    struct oracle_tracker_event *n;
    if (cap > ORACLE_EVENTS_MAX
        || (n = (struct oracle_tracker_event *)realloc(
                events, cap * sizeof *events))
               == NULL) {
      events_overflow = 1;
      return;
    }
    events = n;
    events_cap = cap;
  }
  events[events_len++] = *e;
}

int
oracle_xp_fprintf(FILE *stream, const char *fmt, ...) {
  static const char tag[] = "expat: Allocations(";
  va_list ap;
  int r = 0;
  va_start(ap, fmt);
  if (capture_on && strncmp(fmt, tag, sizeof tag - 1) == 0) {
    /* expat_heap_stat's arguments, in order (xmlparse.c 851-856). */
    const struct XML_ParserStruct *root
        = (const struct XML_ParserStruct *)va_arg(ap, void *);
    struct oracle_tracker_event e;
    memset(&e, 0, sizeof e);
    e.direct = va_arg(ap, unsigned long long);
    e.op = (char)va_arg(ap, int);
    e.size = (unsigned long long)va_arg(ap, size_t);
    e.total = (unsigned long long)va_arg(ap, size_t);
    e.peak = (unsigned long long)va_arg(ap, size_t);
    (void)va_arg(ap, double);
    e.line = va_arg(ap, int);
    /* A refusal is reported by expat_heap_increase_tolerable before the
       count changes, so the reported total is not the live count; every
       granted change is reported after the count was updated. */
    if (e.op == '+' && e.total != root->m_alloc_tracker.bytesAllocated) {
      e.refused = 1;
      e.total = root->m_alloc_tracker.bytesAllocated;
      e.peak = root->m_alloc_tracker.peakBytesAllocated;
    }
    push_event(&e);
  } else {
    r = vfprintf(stream, fmt, ap);
  }
  va_end(ap);
  return r;
}

void
oracle_tracker_events_capture(int on) {
  free(events);
  events = NULL;
  events_len = 0;
  events_cap = 0;
  events_overflow = 0;
  events_at_end = 0;
  capture_on = on;
}

void
oracle_tracker_events_mark_end(void) {
  events_at_end = events_len;
}

size_t
oracle_tracker_events_take(struct oracle_tracker_event **out, int *complete,
                           size_t *at_end) {
  size_t n = events_len;
  *out = events;
  *complete = ! events_overflow;
  *at_end = events_at_end;
  events = NULL;
  events_len = 0;
  events_cap = 0;
  events_overflow = 0;
  events_at_end = 0;
  capture_on = 0;
  return n;
}
