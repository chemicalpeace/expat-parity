#!/usr/bin/env python3
"""Writes "ID<TAB>PATH" lines from OASIS XML catalogs (first entry wins)."""
import sys, os, xml.etree.ElementTree as ET
NS='{urn:oasis:names:tc:entity:xmlns:xml:catalog}'
seen={}
def walk(path):
    path=os.path.abspath(path)
    try: root=ET.parse(path).getroot()
    except Exception: return
    d=os.path.dirname(path)
    for el in root.iter():
        tag=el.tag.replace(NS,'')
        if tag=='public' and el.get('publicId'):
            seen.setdefault(el.get('publicId'), os.path.normpath(os.path.join(d, el.get('uri'))))
        elif tag=='system' and el.get('systemId'):
            seen.setdefault(el.get('systemId'), os.path.normpath(os.path.join(d, el.get('uri'))))
        elif tag=='nextCatalog':
            walk(os.path.join(d, el.get('catalog')))
for c in sys.argv[1:]: walk(c)
for k,v in seen.items(): print(f"{k}\t{v}")
