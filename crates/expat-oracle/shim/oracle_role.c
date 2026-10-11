/* Build shim (not part of upstream): white-box access to xmlrole.c.
 *
 * The prolog state machine keeps its state as a pointer to one of the static
 * handler functions of xmlrole.c. This translation unit textually includes
 * the unmodified vendored xmlrole.c (its two exported functions renamed so
 * this copy does not collide with the one linked into the full parser) and
 * reports the handler as an index into the table below, which lists the
 * handlers in the order of their declaration (mirrored by
 * expat::role::Handler::ALL). Nothing in vendor/ is modified (D-009).
 */
#define XmlPrologStateInit oracle_r_XmlPrologStateInit
#define XmlPrologStateInitExternalEntity oracle_r_XmlPrologStateInitExternalEntity

#include "xmlrole.c"

#include <string.h>

#include "oracle.h"

static PROLOG_HANDLER *const oracle_handlers[] = {
    prolog0,   prolog1,   prolog2,   doctype0,  doctype1,
    doctype2,  doctype3,  doctype4,  doctype5,  internalSubset,
    entity0,   entity1,   entity2,   entity3,   entity4,
    entity5,   entity6,   entity7,   entity8,   entity9,
    entity10,  notation0, notation1, notation2, notation3,
    notation4, attlist0,  attlist1,  attlist2,  attlist3,
    attlist4,  attlist5,  attlist6,  attlist7,  attlist8,
    attlist9,  element0,  element1,  element2,  element3,
    element4,  element5,  element6,  element7,  externalSubset0,
    externalSubset1, condSect0, condSect1, condSect2, declClose,
    error};

static int
handler_index(const PROLOG_STATE *s) {
  size_t i;
  for (i = 0; i < sizeof(oracle_handlers) / sizeof(oracle_handlers[0]); i++)
    if (s->handler == oracle_handlers[i])
      return (int)i;
  return -1;
}

static void
snapshot(const PROLOG_STATE *s, struct oracle_role_state *out) {
  out->handler = handler_index(s);
  out->level = s->level;
  out->role_none = s->role_none;
  out->include_level = s->includeLevel;
  out->document_entity = s->documentEntity;
}

int
oracle_role_run(int external, const struct oracle_role_state *init,
                const void *encv, const char *buf, size_t n, const int *toks,
                const size_t *ptrs, const size_t *ends, int *roles,
                struct oracle_role_state *states) {
  const ENCODING *enc = (const ENCODING *)encv;
  PROLOG_STATE s;
  size_t i;
  memset(&s, 0, sizeof s);
  oracle_r_XmlPrologStateInit(&s);
  if (external)
    oracle_r_XmlPrologStateInitExternalEntity(&s);
  if (init) {
    if (init->handler < 0
        || (size_t)init->handler
               >= sizeof(oracle_handlers) / sizeof(oracle_handlers[0]))
      return -1;
    s.handler = oracle_handlers[init->handler];
    s.level = init->level;
    s.role_none = init->role_none;
    s.includeLevel = init->include_level;
    s.documentEntity = init->document_entity;
  }
  for (i = 0; i < n; i++) {
    roles[i] = XmlTokenRole(&s, toks[i], buf + ptrs[i], buf + ends[i], enc);
    snapshot(&s, &states[i]);
  }
  return 0;
}
