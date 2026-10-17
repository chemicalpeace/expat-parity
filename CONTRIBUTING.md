# Contributing

## One-command check

```sh
scripts/ci-local.sh        # everything CI runs, except the OS matrix
```

## Rules (enforced by CI)

- The C library is the oracle: a behaviour change in the port must be justified
  by the XML spec and recorded in `docs/DIVERGENCES.md` with a reproducer and a
  regression test.
- Verification before features. Never weaken, skip, or `#[ignore]` a check.
- No `todo!`, `unimplemented!`, `TODO`/`FIXME`/`XXX`/`HACK`, commented-out code,
  unjustified `#[allow]`, `unwrap`/`expect` in library code, or panics reachable
  from untrusted input. `scripts/debt-check.sh` enforces this.
- `unsafe` is forbidden in `crates/expat`; it may only appear in
  `crates/expat-oracle` (test-only FFI), each block with a `SAFETY:`
  comment. (An optional C ABI crate, `expat-capi`, is planned as module 13
  in `docs/STATE.md` but does not exist.)
- Never modify `crates/expat-oracle/vendor/`. Build adaptations go in `shim/`
  and are documented in `docs/DECISIONS.md`.
- Small commits whose message says *why*. Update `docs/STATE.md` in the same
  commit when the state changes.
