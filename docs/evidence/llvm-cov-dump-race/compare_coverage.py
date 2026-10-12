#!/usr/bin/env python3
"""Compare CI coverage artifacts (coverage-<OS>/stable) and coverage-job logs
between runs, to check that `-num-threads=1` on the llvm-cov -dump step
changed nothing but that step.

usage: compare_coverage.py LABEL=ARTIFACT_DIR[,JOB_LOG] ...

For each report (all-features-raw, all-features, full-raw, full) it prints,
per run:
  - totals from <report>-summary.json: lines, regions, functions (covered/count)
  - covered-set hash of <report>.lcov: every DA/FNDA/BRDA record reduced to
    covered yes/no, paths cut to crates/..., records sorted (llvm-cov's
    function order and the hit counts vary run to run: hash-map order and
    the unseeded proptest cases)
  - raw hash of the sorted lcov records with hit counts
  - hash of the sorted hash-mismatch list (<report>-mismatch.txt, the output
    of the -dump step)
and for the job log: the set and multiset of `test NAME ... ok` lines and the
summed `test result:` counts.
"""
import hashlib, json, os, re, sys


def h(lines):
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()[:12]


def lcov(path):
    cur, cov, raw = None, [], []
    for l in open(path, encoding="utf-8", errors="replace"):
        l = l.strip()
        if l.startswith("SF:"):
            s = l[3:].replace("\\", "/")
            cur = s[s.find("crates/"):]
            continue
        raw.append(f"{cur}|{l}")
        if l.startswith("DA:"):
            ln, c = l[3:].split(",")[:2]
            l = f"DA:{ln},{int(c) > 0}"
        elif l.startswith("FNDA:"):
            c, n = l[5:].split(",", 1)
            l = f"FNDA:{int(c) > 0},{n}"
        elif l.startswith("BRDA:"):
            p = l[5:].split(",")
            l = "BRDA:" + ",".join(p[:3] + [str(p[3] not in ("-", "0"))])
        cov.append(f"{cur}|{l}")
    return h(sorted(cov)), h(sorted(raw))


def totals(path):
    t = json.load(open(path))["data"][0]["totals"]
    return "lines %d/%d regions %d/%d functions %d/%d" % tuple(
        t[k][x] for k in ("lines", "regions", "functions") for x in ("covered", "count"))


def mismatch(path):
    return h(sorted(l.strip().replace("\\", "/") for l in open(path)))


def tests(log):
    t = open(log, encoding="utf-8", errors="replace").read()
    names = sorted(re.findall(r"^\S+ test (.+?) \.\.\. ok$", t, re.M))
    bad = re.findall(r"^\S+ test (.+?) \.\.\. (FAILED|ignored)$", t, re.M)
    res = re.findall(r"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored", t)
    s = [sum(int(r[i]) for r in res) for i in range(3)]
    return (f"tests ok {len(names)} (distinct {len(set(names))}) set {h(sorted(set(names)))} "
            f"multiset {h(names)}; passed {s[0]} failed {s[1]} ignored {s[2]}; not-ok {bad}")


runs = []
for a in sys.argv[1:]:
    label, rest = a.split("=", 1)
    d, _, log = rest.partition(",")
    runs.append((label, d, log))

for rep in ("all-features-raw", "all-features", "full-raw", "full"):
    for key, fn in (("totals", lambda d: totals(f"{d}/{rep}-summary.json")),
                    ("covered-set", lambda d: lcov(f"{d}/{rep}.lcov")[0]),
                    ("raw lcov (hit counts)", lambda d: lcov(f"{d}/{rep}.lcov")[1]),
                    ("hash-mismatch list", lambda d: mismatch(f"{d}/{rep}-mismatch.txt"))):
        vals = [(l, fn(d)) for l, d, _ in runs]
        same = len({v for _, v in vals}) == 1
        print(f"{rep:17s} {key:22s} {'IDENTICAL' if same else 'DIFFERENT'}")
        for l, v in vals:
            print(f"    {l:10s} {v}")
for l, _, log in runs:
    if log:
        print(f"{l:10s} {tests(log)}")
