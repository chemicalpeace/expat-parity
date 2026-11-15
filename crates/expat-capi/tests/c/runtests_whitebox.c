/* Stand-ins for the libexpat internals that libexpat's test suite
   (tests/runtests, built with XML_TESTING) calls directly: the hash
   table, the internal scanners and helpers of xmlparse.c and the
   library's internal allocation wrappers. They are not part of expat.h
   and have no counterpart in this library, so each one fails the test
   that calls it; scripts/runtests-check.sh lists those tests as white-box
   (D-060). The xcs helpers come from the vendored xcs.c. */
#include "expat.h"
#include "minicheck.h"

#define WHITE_BOX(name)                                                        \
  void name(void) {                                                            \
    _fail(__FILE__, __LINE__, "white-box: " #name);                            \
  }

WHITE_BOX(_INTERNAL_trim_to_complete_utf8_characters)
WHITE_BOX(expat_malloc)
WHITE_BOX(expat_realloc)
WHITE_BOX(expat_free)
WHITE_BOX(hashTableInit)
WHITE_BOX(hashTableClear)
WHITE_BOX(hashTableDestroy)
WHITE_BOX(hashTableIterInit)
WHITE_BOX(hashTableIterNext)
WHITE_BOX(isCDataCommentOrPIStart)
WHITE_BOX(isPredefinedEntityName)
WHITE_BOX(keyeq)
WHITE_BOX(keylen)
WHITE_BOX(lookup)
WHITE_BOX(lookupWithLength)
WHITE_BOX(scanEntityText)
WHITE_BOX(unsignedCharToPrintable)
WHITE_BOX(xmlSetHashSalt)
