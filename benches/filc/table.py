#!/usr/bin/env python3
"""Turns run.sh output into a Markdown table and checks that every
implementation reported the same event counts.

Usage: table.py RESULTS_FILE
"""
import collections
import sys

COUNTS = ("starts", "ends", "chars", "charbytes")


def main():
    rows = collections.OrderedDict()
    for line in open(sys.argv[1]):
        kv = dict(f.split("=", 1) for f in line.split())
        key = (kv["file"], kv["mode"], "single" if kv["chunk"] == "0" else "64 KiB")
        rows.setdefault(key, {})[kv["bin"]] = kv  # the last run of a cell wins
    bad = []
    print("| corpus | handlers | chunks | port LTO MB/s | port release MB/s "
          "| C clang MB/s | C gcc MB/s | Fil-C MB/s | port/Fil-C | port/C clang | C clang/Fil-C |")
    print("|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|")
    for (f, mode, chunk), r in rows.items():
        counts = {b: tuple(v[c] for c in COUNTS) for b, v in r.items()}
        if len(set(counts.values())) != 1:
            bad.append((f, mode, chunk, counts))
        m = {b: float(v["mbps"]) for b, v in r.items()}
        port = min(m.get("rust-lto", 0), m.get("rust-release", 0))
        filc, clang = m.get("c-filc", 0), m.get("c-clang", 0)
        ratio = lambda a, b: f"{a / b:.2f}x" if b else "n/a"
        print(f"| {f} | {mode} | {chunk} | {m.get('rust-lto', 0):.1f} | {m.get('rust-release', 0):.1f} "
              f"| {clang:.1f} | {m.get('c-gcc', 0):.1f} | {filc:.1f} | {ratio(port, filc)} "
              f"| {ratio(port, clang)} | {ratio(clang, filc)} |")
    print()
    if bad:
        print("EVENT COUNT MISMATCH:")
        for b in bad:
            print("  ", b)
        sys.exit(1)
    print(f"Event counts identical across implementations in all {len(rows)} cells.")


if __name__ == "__main__":
    main()
