/* Parses real documents with their DTDs and external entities through
 * expat.h and reports, for each, the outcome under libexpat's default
 * limits and the smallest amplification factors under which it still
 * parses, with libexpat's default activation thresholds and with the
 * factors applied from the first byte (ROADMAP point 3, D-063). Built once against libexpat and once
 * against expat-capi by scripts/doc-limits.sh; the two outputs must be
 * equal.
 *
 * Usage: measure CATALOG [-q] DOC...
 * With -q, the documents after it are only parsed with the defaults.
 * CATALOG has lines "ID<TAB>PATH": a public or system identifier and the
 * local file it stands for. Other system identifiers are resolved
 * relative to the base of the entity that refers to them.
 */
#include <expat.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_IDS 4096

static char *ids[MAX_IDS];
static char *paths[MAX_IDS];
static int n_ids;

static void load_catalog(const char *file) {
  FILE *f = fopen(file, "r");
  char line[4096];
  if (!f) {
    perror(file);
    exit(2);
  }
  while (fgets(line, sizeof line, f) && n_ids < MAX_IDS) {
    char *tab = strchr(line, '\t');
    if (!tab)
      continue;
    *tab = 0;
    line[strcspn(tab + 1, "\n") + (tab + 1 - line)] = 0;
    ids[n_ids] = strdup(line);
    paths[n_ids] = strdup(tab + 1);
    n_ids++;
  }
  fclose(f);
}

static const char *lookup(const char *id) {
  for (int i = 0; id && i < n_ids; i++)
    if (strcmp(ids[i], id) == 0)
      return paths[i];
  return NULL;
}

static char *read_file(const char *path, long *len) {
  FILE *f = fopen(path, "rb");
  char *buf;
  if (!f)
    return NULL;
  fseek(f, 0, SEEK_END);
  *len = ftell(f);
  fseek(f, 0, SEEK_SET);
  buf = malloc(*len ? *len : 1);
  if (fread(buf, 1, *len, f) != (size_t)*len) {
    free(buf);
    buf = NULL;
  }
  fclose(f);
  return buf;
}

static long bytes_read;
static int verbose;
static int missing;

static void resolve(const char *base, const char *system_id,
                    const char *public_id, char *out, size_t size) {
  const char *p = lookup(public_id);
  if (!p)
    p = lookup(system_id);
  if (p) {
    snprintf(out, size, "%s", p);
  } else if (strstr(system_id, "://") || system_id[0] == '/' || !base) {
    snprintf(out, size, "%s", system_id);
  } else {
    const char *slash = strrchr(base, '/');
    int dir = slash ? (int)(slash - base + 1) : 0;
    snprintf(out, size, "%.*s%s", dir, base, system_id);
  }
}

static int parse_file(XML_Parser p, const char *path) {
  long len;
  char *buf = read_file(path, &len);
  int status;
  if (!buf) {
    missing++;
    return XML_STATUS_ERROR;
  }
  bytes_read += len;
  XML_SetBase(p, path);
  status = XML_Parse(p, buf, (int)len, XML_TRUE);
  free(buf);
  return status;
}

static int XMLCALL external(XML_Parser p, const XML_Char *context,
                            const XML_Char *base, const XML_Char *system_id,
                            const XML_Char *public_id) {
  char path[4096];
  XML_Parser ext;
  int status;
  if (!system_id)
    return XML_STATUS_OK;
  resolve(base, system_id, public_id, path, sizeof path);
  ext = XML_ExternalEntityParserCreate(p, context, NULL);
  if (!ext)
    return XML_STATUS_ERROR;
  status = parse_file(ext, path);
  if (status != XML_STATUS_OK && verbose)
    fprintf(stderr, "  %s: %s at line %lu\n", path,
            XML_ErrorString(XML_GetErrorCode(ext)),
            (unsigned long)XML_GetCurrentLineNumber(ext));
  XML_ParserFree(ext);
  return status;
}

enum limit { NONE, AMPLIFICATION, TRACKER };

/* Parses DOC; with a limit, sets that factor, and with EARLY an
 * activation threshold of one byte, so the factor applies from the first
 * byte instead of after libexpat's default threshold. */
static int early;

static int run(const char *doc, enum limit which, float factor,
               enum XML_Error *code, XML_Size *line) {
  XML_Parser p = XML_ParserCreate(NULL);
  int ok;
  XML_SetParamEntityParsing(p, XML_PARAM_ENTITY_PARSING_ALWAYS);
  XML_SetExternalEntityRefHandler(p, external);
  /* Each search lifts the other limit, to measure one at a time. */
  if (which == AMPLIFICATION)
    XML_SetAllocTrackerMaximumAmplification(p, 1e12f);
  if (which == TRACKER)
    XML_SetBillionLaughsAttackProtectionMaximumAmplification(p, 1e12f);
  if (which == AMPLIFICATION) {
    if (early)
      XML_SetBillionLaughsAttackProtectionActivationThreshold(p, 1);
    XML_SetBillionLaughsAttackProtectionMaximumAmplification(p, factor);
  } else if (which == TRACKER) {
    if (early)
      XML_SetAllocTrackerActivationThreshold(p, 1);
    XML_SetAllocTrackerMaximumAmplification(p, factor);
  }
  ok = parse_file(p, doc) == XML_STATUS_OK;
  if (code)
    *code = XML_GetErrorCode(p);
  if (line)
    *line = XML_GetCurrentLineNumber(p);
  XML_ParserFree(p);
  return ok;
}

/* The smallest factor (to 0.1%) under which DOC parses, or 0 when it does
 * not parse even at 1e9. */
static double smallest(const char *doc, enum limit which) {
  double lo = 1.0, hi = 1e9;
  if (run(doc, which, (float)lo, NULL, NULL))
    return lo;
  if (!run(doc, which, (float)hi, NULL, NULL))
    return 0;
  while (hi / lo > 1.001) {
    double mid = sqrt(lo * hi);
    if (run(doc, which, (float)mid, NULL, NULL))
      hi = mid;
    else
      lo = mid;
  }
  return hi;
}

/* A factor as printed: "above-1e9" when no factor up to 1e9 suffices. */
static void factor(const char *name, double f) {
  if (f == 0)
    printf(" %s=above-1e9", name);
  else
    printf(" %s=%.3f", name, f);
}

int main(int argc, char **argv) {
  if (argc < 3) {
    fprintf(stderr, "usage: measure CATALOG [-q] DOC...\n");
    return 2;
  }
  int quick = 0;
  load_catalog(argv[1]);
  for (int i = 2; i < argc; i++) {
    if (strcmp(argv[i], "-q") == 0) {
      quick = 1;
      continue;
    }
    enum XML_Error code;
    XML_Size line;
    int ok;
    bytes_read = 0;
    missing = 0;
    verbose = 1;
    ok = run(argv[i], NONE, 0, &code, &line);
    verbose = 0;
    printf("%s bytes=%ld", argv[i], bytes_read);
    if (ok)
      printf(" ok");
    else
      printf(" error=%d line=%lu missing=%d", (int)code, (unsigned long)line,
             missing);
    if (quick) {
      printf("\n");
      continue;
    }
    /* The smallest factors with the default thresholds, then from the
     * first byte. */
    early = 0;
    factor("amplification", smallest(argv[i], AMPLIFICATION));
    factor("tracker", smallest(argv[i], TRACKER));
    early = 1;
    printf(" from-first-byte");
    factor("amplification", smallest(argv[i], AMPLIFICATION));
    factor("tracker", smallest(argv[i], TRACKER));
    printf("\n");
    early = 0;
  }
  return 0;
}
