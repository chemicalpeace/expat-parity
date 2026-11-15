#!/usr/bin/env python3
"""Generates the synthetic benchmark corpus (deterministic, seeded).

Usage: gen_corpus.py OUT_DIR

Writes:
  bench-mixed.xml  ~8 MB  libexpat tests/benchmark-style mixed document:
                          elements, attributes, comments, PIs, CDATA,
                          predefined entity and character references.
  plain-large.xml  ~50 MB many small elements with several attributes each.
  ns-heavy.xml     ~12 MB several namespaces, prefixed elements and
                          attributes, nested redeclarations (parse with a
                          namespace separator).
  utf16le.xml      ~16 MB the UTF-16LE (BOM) encoding of an ~8 MB
                          document with non-ASCII text.
  docbook-dtd.xml  ~10 MB DocBook-like document with an internal DTD
                          subset: element/attlist declarations, default
                          attributes and internal general entities.
"""
import os
import random
import sys

WORDS = ("lorem ipsum dolor sit amet consectetur adipiscing elit sed do "
         "eiusmod tempor incididunt ut labore et dolore magna aliqua").split()


def text(r, n):
    return " ".join(r.choice(WORDS) for _ in range(n))


def write(path, parts):
    with open(path, "wb") as f:
        for p in parts:
            f.write(p.encode("utf-8") if isinstance(p, str) else p)


def bench_mixed(r, target):
    yield "<?xml version='1.0' encoding='UTF-8'?>\n<catalog>\n"
    size, i = 0, 0
    while size < target:
        s = (f"  <!-- record {i} -->\n"
             f"  <record id='r{i}' status=\"{r.choice(['new', 'old', 'mid'])}\" rank='{r.randint(1, 999)}'>\n"
             f"    <title>{text(r, 5)} &amp; {text(r, 2)}</title>\n"
             f"    <body>{text(r, 20)} &lt;tag&gt; &#169; &#x263A; {text(r, 8)}</body>\n"
             f"    <?render mode='fast'?>\n"
             f"    <code><![CDATA[if (a < b && c > d) {{ x = \"{i}\"; }}]]></code>\n"
             f"    <tags><t>{r.choice(WORDS)}</t><t>{r.choice(WORDS)}</t><t/></tags>\n"
             f"  </record>\n")
        size += len(s)
        i += 1
        yield s
    yield "</catalog>\n"


def plain_large(r, target):
    yield "<?xml version='1.0'?>\n<root>\n"
    size, i = 0, 0
    while size < target:
        s = (f"<item id='{i}' a='{r.randint(0, 99999)}' b=\"{r.choice(WORDS)}\" "
             f"c='{r.choice(WORDS)}-{i % 97}' d='x'><name>{r.choice(WORDS)}</name>"
             f"<v k='{i % 13}'>{r.randint(0, 10**9)}</v><e/></item>\n")
        size += len(s)
        i += 1
        yield s
    yield "</root>\n"


def ns_heavy(r, target):
    yield ("<?xml version='1.0'?>\n<a:root xmlns:a='urn:example:a' "
           "xmlns:b='http://example.com/ns/b' xmlns='urn:example:default' "
           "xmlns:xl='http://www.w3.org/1999/xlink'>\n")
    size, i = 0, 0
    while size < target:
        s = (f"<b:entry b:id='{i}' a:kind='{r.choice(WORDS)}' xl:href='#n{i}'>"
             f"<a:t>{text(r, 3)}</a:t>"
             f"<c:x xmlns:c='urn:example:c{i % 7}' c:v='{i}'><c:y a:z='1'/></c:x>"
             f"<plain attr='{i % 11}'>{r.choice(WORDS)}</plain>"
             f"</b:entry>\n")
        size += len(s)
        i += 1
        yield s
    yield "</a:root>\n"


def utf16_doc(r, target):
    parts = ["<?xml version='1.0' encoding='UTF-16'?>\n<doc>\n"]
    size, i = 0, 0
    while size < target:
        s = (f"<p n='{i}' lang='{r.choice(['tr', 'de', 'el'])}'>"
             f"{text(r, 6)} Çağrı ğüşıöç straße Ωμέγα {r.randint(0, 9999)}</p>\n")
        size += len(s.encode("utf-8"))
        i += 1
        parts.append(s)
    parts.append("</doc>\n")
    return [b"\xff\xfe" + "".join(parts).encode("utf-16-le")]


def docbook(r, target):
    yield """<?xml version='1.0'?>
<!DOCTYPE book [
  <!ELEMENT book (title, chapter+)>
  <!ELEMENT chapter (title, para+)>
  <!ELEMENT title (#PCDATA)>
  <!ELEMENT para (#PCDATA|emphasis|link)*>
  <!ELEMENT emphasis (#PCDATA)>
  <!ELEMENT link (#PCDATA)>
  <!ATTLIST book lang CDATA 'en' status (draft|final) 'draft'>
  <!ATTLIST chapter id ID #REQUIRED role CDATA 'body' xreflabel CDATA #IMPLIED>
  <!ATTLIST para role CDATA 'normal' revision CDATA '1'>
  <!ATTLIST emphasis role CDATA 'italic'>
  <!ATTLIST link linkend IDREF #REQUIRED type CDATA 'internal'>
  <!ENTITY product 'Expat Parity Reference Manual'>
  <!ENTITY vendor 'The Example Project &amp; Friends'>
  <!ENTITY copy '&#169;'>
  <!ENTITY note 'Note: see &product; by &vendor; &copy; 2026.'>
]>
<book>
<title>&product;</title>
"""
    size, i = 0, 0
    while size < target:
        paras = "".join(
            f"<para>{text(r, 12)} <emphasis>{r.choice(WORDS)}</emphasis> &product; "
            f"{text(r, 6)} <link linkend='c{max(i - 1, 0)}'>{r.choice(WORDS)}</link> &note;</para>\n"
            for _ in range(4))
        s = f"<chapter id='c{i}'>\n<title>Chapter {i} &vendor;</title>\n{paras}</chapter>\n"
        size += len(s)
        i += 1
        yield s
    yield "</book>\n"


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    mb = 1 << 20
    write(os.path.join(out, "bench-mixed.xml"), bench_mixed(random.Random(1), 8 * mb))
    write(os.path.join(out, "plain-large.xml"), plain_large(random.Random(2), 50 * mb))
    write(os.path.join(out, "ns-heavy.xml"), ns_heavy(random.Random(3), 12 * mb))
    write(os.path.join(out, "utf16le.xml"), utf16_doc(random.Random(4), 8 * mb))
    write(os.path.join(out, "docbook-dtd.xml"), docbook(random.Random(5), 10 * mb))


if __name__ == "__main__":
    main()
