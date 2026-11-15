# Following libexpat releases

The port is pinned to one libexpat release (currently 2.10.0, D-042) and
compared with a vendored copy of it. This document is the procedure for
moving to a new libexpat release, and for the weekly comparison with
libexpat's development branch (D-062). D-042, the move from 2.9.0 to
2.10.0, is the worked example.

## Weekly comparison with the development branch

`.github/workflows/upstream.yml` runs `scripts/upstream-check.sh` every
Monday and on demand. The script:

1. fetches `master` of https://github.com/libexpat/libexpat (another
   branch or tag with `UPSTREAM_REF`, an existing checkout with
   `UPSTREAM_SRC`);
2. lists the files of `expat/lib/` that differ from the vendored copy;
3. runs `scripts/capi-check.sh` with the branch as the C side
   (`EXPAT_LIB`), so the event traces of a C program built against the
   branch and against `expat-capi` are compared on every test vector;
4. runs `scripts/runtests-check.sh` with the branch's own test suite
   (`EXPAT_SRC`, `EXPAT_LIB`) against the drop-in library.

The job does not block pull requests. When it fails, read its log: a
changed trace or a newly failing test is a change in libexpat the port
does not follow yet. Open an issue in this repository that names the
libexpat commit or pull request, and port it with the release that
contains it (below). A test that fails only because it calls a new
libexpat internal is added to
`crates/expat-capi/tests/runtests-white-box.txt` with that release.

## Moving to a new libexpat release

1. **Read the release.** Go through libexpat's `Changes` file and the
   merged pull requests between the two tags. For each change note
   whether it is observable through `expat.h` (events, error codes,
   positions, allocator calls, accounting) or internal only.
2. **Re-vendor.** Replace `crates/expat-oracle/vendor/expat` with the
   files of the new tag, byte for byte, in a commit of its own, and
   update `crates/expat-oracle/vendor.sha256` and the commit named in
   `NOTICE`. `scripts/verify-vendor.sh` must pass.
3. **Run the comparison.** `cargo test --release --workspace`,
   `scripts/capi-check.sh`, `scripts/runtests-check.sh` (with the new
   tag's URL and SHA-256 in the script), `scripts/w3c-check.sh`, the
   allocation failure comparison (`oom_diff.rs`) and the structure sizes
   (`tracker_sizes.rs`). Every difference is either a change to port or
   a test that pinned the old behaviour.
4. **Port the changes.** Port each observable change, with a test that
   fails before and passes after. Regenerate
   `tests/vectors/upstream.traces.jsonl` when the traces change, and add
   the release's new upstream tests to the vectors
   (`scripts/extract_vectors.py`). Changes with no observable effect in
   the port are listed, with the reason, rather than ported.
5. **Security fixes.** For every CVE fixed in the release, add a test to
   the CVE suite that shows the port's behaviour on the published
   reproducer.
6. **Record.** One decision in `docs/DECISIONS.md` in the form of D-042
   (what the port follows, what is not ported and why, consequences for
   tests and documents); update the version in the README, the crate
   docs, `XML_ExpatVersion` and `XML_GetFeatureList` in `expat-capi`,
   `docs/STATE.md` and `docs/ROADMAP.md`; add a `CHANGELOG.md` entry
   naming the libexpat version and its CVEs.
7. **Release the crate.** Raise the crate version (a libexpat security
   release is a patch release of the port when the API does not change),
   run `scripts/package-check.sh`, tag `vX.Y.Z`, and publish only after
   CI is green on the tag.

## Timing

A libexpat security release is followed first: the vendored copy and the
port's fix are prepared as soon as the release is public. Other releases
are followed in the order they come.
