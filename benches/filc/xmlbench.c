/* Throughput harness for C libexpat (the vendored 2.10.0 sources of
 * crates/expat-oracle), built by benches/filc/run.sh with a plain C compiler
 * and with Fil-C. The Rust twin is benches/src/bin/xmlbench.rs; both take the
 * same arguments and print the same line, so the counts can be compared.
 *
 *   xmlbench FILE MODE CHUNK NS RUNS
 *
 * MODE  none   no handlers set
 *       count  start/end element and character data handlers that count
 * CHUNK 0 = the whole document in one XML_Parse call, otherwise the size of
 *       each XML_Parse call (the last one is final)
 * NS    1 = XML_ParserCreateNS(NULL, '|'), 0 = XML_ParserCreate(NULL)
 * RUNS  timed runs after one untimed warm-up run
 *
 * Output: impl=c bytes=N runs=R median_ns=M min_ns=m mbps=X starts=S ends=E
 *         chars=C charbytes=B
 * The document is read into memory once; each run creates a parser, parses
 * the document and frees the parser. The salt is fixed (no entropy reads).
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "expat.h"

struct counts {
  unsigned long long starts, ends, chars, charbytes;
};

static void XMLCALL
on_start(void *ud, const XML_Char *name, const XML_Char **atts) {
  (void)name;
  (void)atts;
  ((struct counts *)ud)->starts++;
}

static void XMLCALL
on_end(void *ud, const XML_Char *name) {
  (void)name;
  ((struct counts *)ud)->ends++;
}

static void XMLCALL
on_chars(void *ud, const XML_Char *s, int len) {
  (void)s;
  ((struct counts *)ud)->chars++;
  ((struct counts *)ud)->charbytes += (unsigned long long)len;
}

static uint64_t
now_ns(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (uint64_t)ts.tv_sec * 1000000000u + (uint64_t)ts.tv_nsec;
}

static int
cmp_u64(const void *a, const void *b) {
  uint64_t x = *(const uint64_t *)a, y = *(const uint64_t *)b;
  return x < y ? -1 : x > y;
}

static int
run_once(const char *doc, size_t len, int count, size_t chunk, int ns,
         struct counts *c) {
  static const uint8_t salt[16] = {7, 7, 7, 7, 7, 7, 7, 7,
                                   7, 7, 7, 7, 7, 7, 7, 7};
  XML_Parser p = ns ? XML_ParserCreateNS(NULL, '|') : XML_ParserCreate(NULL);
  if (! p)
    return 0;
  XML_SetHashSalt16Bytes(p, salt);
  memset(c, 0, sizeof *c);
  if (count) {
    XML_SetUserData(p, c);
    XML_SetElementHandler(p, on_start, on_end);
    XML_SetCharacterDataHandler(p, on_chars);
  }
  int ok = 1;
  if (chunk == 0) {
    ok = XML_Parse(p, doc, (int)len, XML_TRUE) == XML_STATUS_OK;
  } else {
    for (size_t off = 0; ok && off < len; off += chunk) {
      size_t n = len - off < chunk ? len - off : chunk;
      int final = off + n == len;
      ok = XML_Parse(p, doc + off, (int)n, final) == XML_STATUS_OK;
    }
  }
  if (! ok)
    fprintf(stderr, "parse error: %s at line %lu\n",
            XML_ErrorString(XML_GetErrorCode(p)),
            (unsigned long)XML_GetCurrentLineNumber(p));
  XML_ParserFree(p);
  return ok;
}

int
main(int argc, char **argv) {
  if (argc != 6) {
    fprintf(stderr, "usage: %s FILE none|count CHUNK NS RUNS\n", argv[0]);
    return 2;
  }
  FILE *f = fopen(argv[1], "rb");
  if (! f) {
    perror(argv[1]);
    return 2;
  }
  fseek(f, 0, SEEK_END);
  long flen = ftell(f);
  fseek(f, 0, SEEK_SET);
  char *doc = malloc((size_t)flen);
  if (! doc || fread(doc, 1, (size_t)flen, f) != (size_t)flen) {
    fprintf(stderr, "read failed\n");
    return 2;
  }
  fclose(f);
  size_t len = (size_t)flen;
  int count = strcmp(argv[2], "count") == 0;
  size_t chunk = (size_t)strtoull(argv[3], NULL, 10);
  int ns = atoi(argv[4]);
  int runs = atoi(argv[5]);
  if (runs < 1 || runs > 1000)
    return 2;

  struct counts c;
  if (! run_once(doc, len, count, chunk, ns, &c))
    return 1;
  uint64_t *t = malloc(sizeof(uint64_t) * (size_t)runs);
  for (int i = 0; i < runs; i++) {
    uint64_t t0 = now_ns();
    if (! run_once(doc, len, count, chunk, ns, &c))
      return 1;
    t[i] = now_ns() - t0;
  }
  qsort(t, (size_t)runs, sizeof *t, cmp_u64);
  uint64_t med = runs % 2 ? t[runs / 2] : (t[runs / 2 - 1] + t[runs / 2]) / 2;
  printf("impl=c bytes=%zu runs=%d median_ns=%llu min_ns=%llu mbps=%.1f "
         "starts=%llu ends=%llu chars=%llu charbytes=%llu\n",
         len, runs, (unsigned long long)med, (unsigned long long)t[0],
         (double)len / 1048576.0 / ((double)med / 1e9), c.starts, c.ends,
         c.chars, c.charbytes);
  free(t);
  free(doc);
  return 0;
}
