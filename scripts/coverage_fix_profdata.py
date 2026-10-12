#!/usr/bin/env python3
"""Work around an llvm-cov attribution bug that under-reports coverage when
several test binaries are reported together (docs/REPORT.md, "Coverage on
every CI platform").

A test binary that links a function without using it carries a *dummy*
coverage record for it (function hash 0). llvm-cov looks every record up in
the profile by (name, hash). For the dummy, IndexedInstrProfReader returns
`hash_mismatch` only if the profile holds a record of that name whose hash
agrees with 0 in bit 60 (the "context-sensitive" flag, CS_FLAG_IN_FUNC_HASH);
otherwise it returns `unknown_function`. llvm-cov then gives the dummy zero
counts and registers it as the function's record, and the real record of a
binary loaded later is skipped as a duplicate (CoverageMapping.cpp,
RecordProvenance). So a function whose real hash has bit 60 set (about half
of all functions) is reported as never executed whenever an earlier binary
in the -object list has a dummy for it, even though the profile counts it.

Fix: for every function name whose profile hashes all have bit 60 set, add a
record with hash 1 (bit 60 clear) and one counter. No coverage mapping has
hash 1, so the record itself is never used; its only effect is that a dummy
lookup now ends in `hash_mismatch` and the dummy is dropped (like every
other dummy), leaving the real record to be loaded. The counter is 1, not 0,
because cargo-llvm-cov merges with `-sparse`, which drops all-zero records.

Usage: coverage_fix_profdata.py LLVM_PROFDATA IN.profdata OUT.proftext
Writes the extra records as a text profile. scripts/coverage.sh stores it as
a `.profraw` file next to the raw profiles, so that cargo-llvm-cov's own
`llvm-profdata merge` (which detects the format from the content) includes
it. Prints how many names were augmented.
"""
import re
import subprocess
import sys

CS_BIT = 1 << 60


def main() -> int:
    tool, src, dst = sys.argv[1:4]
    shown = subprocess.run(
        [tool, "show", "--all-functions", src],
        check=True, capture_output=True, text=True,
    ).stdout
    hashes: dict[str, set[int]] = {}
    name = None
    for line in shown.splitlines():
        m = re.match(r"  (\S.*):$", line)
        if m:
            name = m.group(1)
            hashes.setdefault(name, set())
            continue
        m = re.match(r"    Hash: 0x([0-9a-f]+)$", line)
        if m and name is not None:
            hashes[name].add(int(m.group(1), 16))
    names = sorted(n for n, hs in hashes.items() if hs and all(h & CS_BIT for h in hs))
    with open(dst, "w", encoding="ascii") as t:
        for n in names:
            t.write(f"{n}\n# Func Hash:\n1\n# Num Counters:\n1\n# Counter Values:\n1\n\n")
    print(f"profile names: {len(hashes)}; all hashes with bit 60 set (augmented): {len(names)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
