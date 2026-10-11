/* Build shim for the vendored libexpat oracle (not part of upstream).
 *
 * Upstream generates this header with CMake/autotools from
 * expat/expat_config.h.cmake. The oracle is built by `cc` from build.rs, so
 * this hand-written equivalent pins the configuration the Rust port must match:
 *
 *   XML_DTD, XML_GE=1, XML_NS, XML_CONTEXT_BYTES=1024, UTF-8 XML_Char.
 *
 * Entropy: /dev/urandom (Linux, macOS, BSD) or rand_s (Windows, selected by
 * xmlparse.c itself under _WIN32; build.rs compiles the matching file). The oracle always sets a fixed
 * hash salt through XML_SetHashSalt, so the entropy source never influences
 * observable output; it only has to compile. See docs/DECISIONS.md (D-008).
 */
#ifndef EXPAT_CONFIG_H
#define EXPAT_CONFIG_H 1

#include <stdint.h>

#ifndef _WIN32
#  define HAVE_FCNTL_H 1
#  define HAVE_UNISTD_H 1
#  define XML_DEV_URANDOM 1
#endif
#define HAVE_INTTYPES_H 1
#define HAVE_STDINT_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STRING_H 1
#define HAVE_SYS_STAT_H 1
#define HAVE_SYS_TYPES_H 1
#define STDC_HEADERS 1

#define PACKAGE "expat"
#define PACKAGE_NAME "expat"
#define PACKAGE_TARNAME "expat"
#define PACKAGE_URL ""
#define PACKAGE_VERSION "2.9.0"
#define PACKAGE_STRING "expat 2.9.0"
#define PACKAGE_BUGREPORT ""

#define XML_CONTEXT_BYTES 1024
#define XML_DTD 1
#define XML_GE 1
#define XML_NS 1

#endif /* EXPAT_CONFIG_H */
