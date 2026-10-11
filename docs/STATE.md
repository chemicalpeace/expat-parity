# State

Single source of truth for resuming work. Update in the same commit as any
state change. Read with `docs/DECISIONS.md` and `git log`.

## Snapshot

- **Target:** libexpat 2.9.0 (`R_2_9_0`, `a76b1174c1ea52abb639a4c69d52b630a8a106a2`) — frozen (D-001).
- **Phase:** 3 (bottom-up port) — started. Phases 0, 1, 2 complete.
- **Remote:** `chemicalpeace/expat-rs` (intended **public**) — **NOT CREATED: blocked**, see Blockers.
- **CI:** workflow written (`.github/workflows/ci.yml`); runs locally via `scripts/ci-local.sh` (D-012). No GitHub Actions run yet.

## Blockers

1. **Remote repository cannot be created by the agent.** The GitHub integration
   lacks `repository_creation`/`administration` permission (403 "Resource not
   accessible by integration" on `POST /user/repos` and GraphQL
   `createRepository`; installation is on *selected* repositories).
   **Owner action:** create an empty **public** repo `chemicalpeace/expat-rs`
   (no README/license), then add it to the Hark GitHub App's repository access.
   Then: `git remote add origin https://github.com/chemicalpeace/expat-rs.git && git push -u origin main`.

## Environment notes (for a fresh machine)

- Needs a C compiler (`build-essential` or Xcode CLT), rustup. Toolchain is
  pinned by `rust-toolchain.toml` (1.99.0); nightly for Miri/fuzz.
- `cargo install cargo-fuzz --locked` and `cargo-deny` (prebuilt release
  binaries work).
- Builds are faster with `CARGO_TARGET_DIR` on local disk.

## Phase checklist

- [x] Phase 0 — selection, scoring table, freeze (D-001)
- [x] Phase 1 — workspace, vendored oracle (+ `scripts/verify-vendor.sh`), docs, debt check, CI workflow, deny
- [x] Phase 2 — oracle FFI + trace shim; trace spec (ARCHITECTURE.md); 540 upstream vectors with provenance + golden traces; differential harness (`tests/src/diff.rs`); harness self-test (permanent mutation tests + deliberate-bug demonstration, REPORT.md); fuzz target `parse_trace`
- [ ] Phase 3 — bottom-up port (module table below)
- [ ] Phase 4–5 — long fuzzing, coverage ≥95 %, sanitizers, benchmarks, release gates

## Phase 3 modules (bottom-up order, D-004)

| # | Module | C source | Status |
|---|---|---|---|
| 1 | `chartab` byte-type tables | `asciitab.h` `iasciitab.h` `latin1tab.h` `utf8tab.h`, `xmltok_impl.h` | see below |
| 2 | `naming` name-char predicates + UTF-8 validity | `nametab.h`, `xmltok.c` L86-L195 | see below |
| 3 | `encode` UTF-8/UTF-16 encoders | `xmltok.c` `XmlUtf8Encode` `XmlUtf16Encode` | see below |
| 4 | `siphash` | `siphash.h` | not started |
| 5 | `tok` tokenizer, normal encodings | `xmltok_impl.c` | not started |
| 6 | `tok` UTF-16LE/BE + unknown encodings, conversions | `xmltok.c` | not started |
| 7 | `tok` XML decl + encoding detection (`initScan`, Q-3) | `xmltok.c`, `xmltok_ns.c` | not started |
| 8 | `role` prolog state machine | `xmlrole.c` | not started |
| 9 | `parse` pools/hash tables | `xmlparse.c` | not started |
| 10 | `parse` content, attributes, namespaces, epilog (Q-1, Q-2) | `xmlparse.c` | not started |
| 11 | `parse` DTD, entities, amplification limits | `xmlparse.c` | not started |
| 12 | public API `expat::Parser` + trace adapter + differential fuzzing | — | not started |
| 13 | `expat-capi` (optional) | `expat.h` | not started |

## Next steps

1. Get the remote created (Blocker 1), push `main`, watch the first Actions run, fix any OS-matrix issue (macOS untested).
2. Continue Phase 3 at the first module not marked done above.
3. Run ≥10 min fuzzing per touched entry point for each module and record executions in REPORT.md.
