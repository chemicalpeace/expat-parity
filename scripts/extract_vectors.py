#!/usr/bin/env python3
"""Extract XML test documents from libexpat's C test suite.

Usage: scripts/extract_vectors.py <path-to-libexpat-checkout>/expat > tests/vectors/upstream.jsonl

Every C string literal (adjacent literals concatenated, C escapes decoded)
found inside a START_TEST ... END_TEST body that looks like an XML document is
emitted once (deduplicated by bytes) with its provenance (file, line, test).

When the literal is the first argument of `expect_failure(...)` or is paired
with an `XML_ERROR_*` constant in a `{ "...", XML_ERROR_X }` initializer, the
expected error is recorded. `plain` is true when the test body does not
configure the parser (no handlers, encodings, namespaces, foreign DTD, param
entity parsing, or custom parser creation), i.e. when the upstream assertion
applies to a default-configured parser fed the whole document at once.
"""
import json
import os
import re
import sys

CONFIG_MARKERS = re.compile(
    r"XML_Set|XML_ParserCreate|XML_UseForeignDTD|XML_ExternalEntityParserCreate|"
    r"XML_ParserReset\(g_parser, *[^N)]|XML_UseParserAsHandlerArg|XML_SetReparseDeferralEnabled|"
    r"set_subtest_encoding|g_parser *=|_XML_Parse_SINGLE_BYTES\([^,]+,[^,]+,[^,]+, *XML_FALSE"
)


def error_codes(expat_h):
    src = open(expat_h, encoding="utf-8").read()
    body = re.search(r"enum XML_Error \{(.*?)\};", src, re.S).group(1)
    names = re.findall(r"\b(XML_ERROR_[A-Z_0-9]+)", body)
    return {n: i for i, n in enumerate(names)}


ESC = {"n": 10, "t": 9, "r": 13, "a": 7, "b": 8, "f": 12, "v": 11,
       "\\": 92, "'": 39, '"': 34, "?": 63}


def decode_c_string(s):
    out = bytearray()
    i = 0
    while i < len(s):
        c = s[i]
        if c != "\\":
            out += c.encode("utf-8")
            i += 1
            continue
        i += 1
        c = s[i]
        if c in ESC:
            out.append(ESC[c]); i += 1
        elif c == "x":
            j = i + 1
            while j < len(s) and s[j] in "0123456789abcdefABCDEF":
                j += 1
            out.append(int(s[i + 1:j], 16) & 0xFF); i = j
        elif c in "01234567":
            j = i
            while j < len(s) and j < i + 3 and s[j] in "01234567":
                j += 1
            out.append(int(s[i:j], 8) & 0xFF); i = j
        else:
            raise ValueError("unknown escape \\" + c)
    return bytes(out)


LIT = re.compile(r'"((?:[^"\\\n]|\\.)*)"')


def literals(body):
    """Yield (offset, bytes) for each maximal run of adjacent string literals."""
    pos = 0
    while True:
        m = LIT.search(body, pos)
        if not m:
            return
        start = m.start()
        # skip literals that are part of a character constant or a comment line
        parts = [m.group(1)]
        end = m.end()
        while True:
            m2 = re.compile(r'\s*(?:/\*.*?\*/\s*)*"((?:[^"\\\n]|\\.)*)"', re.S).match(body, end)
            if not m2:
                break
            parts.append(m2.group(1)); end = m2.end()
        yield start, end, decode_c_string("".join(parts))
        pos = end


def looks_like_xml(b):
    if len(b) < 2:
        return False
    return (b"<" in b or b"\x00<" in b or b.startswith(b"\xef\xbb\xbf")
            or b.startswith(b"\xff\xfe") or b.startswith(b"\xfe\xff"))


def strip_comments(src):
    # Replace comments by spaces of equal length (keeps offsets / line numbers).
    def blank(m):
        return re.sub(r"[^\n]", " ", m.group(0))
    return re.sub(r"/\*.*?\*/|//[^\n]*", blank, src, flags=re.S)


def main():
    root = sys.argv[1]
    codes = error_codes(os.path.join(root, "lib", "expat.h"))
    seen = {}
    per_line = {}
    out = []
    tests_dir = os.path.join(root, "tests")
    for fname in sorted(os.listdir(tests_dir)):
        if not fname.endswith("_tests.c"):
            continue
        raw = open(os.path.join(tests_dir, fname), encoding="utf-8").read()
        src = strip_comments(raw)
        for tm in re.finditer(r"START_TEST\((\w+)\)(.*?)END_TEST", src, re.S):
            test, body = tm.group(1), tm.group(2)
            body_off = tm.start(2)
            plain = CONFIG_MARKERS.search(body) is None
            for s, e, b in literals(body):
                if not looks_like_xml(b):
                    continue
                before = body[max(0, s - 40):s]
                after = body[e:e + 120]
                expected = None
                m = re.match(r"\s*,\s*(XML_ERROR_[A-Z_0-9]+)", after)
                if m and re.search(r"\{\s*$", before):
                    expected = m.group(1)
                m = re.search(r"expect_failure\(\s*$", before)
                if m:
                    m2 = re.match(r"\s*,\s*(XML_ERROR_[A-Z_0-9]+)", after)
                    if m2:
                        expected = m2.group(1)
                line = raw.count("\n", 0, body_off + s) + 1
                key = b
                if key in seen:
                    seen[key]["dups"] += 1
                    continue
                ordinal = per_line.get((fname, line), 0)
                per_line[(fname, line)] = ordinal + 1
                rec = {
                    "id": "%s:%d" % (fname, line) + ("#%d" % ordinal if ordinal else ""),
                    "source": "expat/tests/%s:%d" % (fname, line),
                    "test": test,
                    "plain": plain,
                    "expected_error": expected,
                    "expected_error_code": codes.get(expected) if expected else None,
                    "doc_hex": b.hex(),
                    "dups": 0,
                }
                seen[key] = rec
                out.append(rec)
    for rec in out:
        sys.stdout.write(json.dumps(rec, sort_keys=True) + "\n")
    sys.stderr.write("extracted %d unique documents, %d with expected errors\n"
                     % (len(out), sum(1 for r in out if r["expected_error"])))


if __name__ == "__main__":
    main()
