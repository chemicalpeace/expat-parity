# Expat-Parity

A memory-safe Rust port of [libexpat](https://github.com/libexpat/libexpat)
2.9.0, the stream-oriented XML parser, verified by differential testing and
fuzzing against the vendored C reference.

> **Status: pre-release, work in progress.** The verification infrastructure is
> in place and the port is being built bottom-up. There is no usable parser API
> yet. See [`docs/STATE.md`](docs/STATE.md) for exactly what is done.

## Why

libexpat is embedded almost everywhere XML is parsed (CPython, browsers, OS
platforms) and has a long CVE record of memory-safety bugs. The aim is a port
whose behaviour is *provably the same* on every input — same events, same
errors, same error positions — without memory-unsafety. How "the same" is
defined and checked is described in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Layout

| Path | What |
|---|---|
| `crates/expat` | the safe port (`#![forbid(unsafe_code)]`, `no_std` + `alloc`) |
| `crates/expat-oracle` | vendored libexpat C at a pinned commit + FFI, **test-only** |
| `tests/` | differential harness, upstream-derived vectors |
| `fuzz/` | cargo-fuzz targets and committed corpus |
| `benches/` | criterion benchmarks |
| `docs/` | decisions, architecture, divergences, report, state |

## Build and verify

Requires a C compiler (for the oracle) and rustup.

```sh
cargo test --workspace            # unit, differential and vector tests
scripts/ci-local.sh               # the full CI gate locally (needs nightly for Miri/fuzz)
```

## License

MIT, same as libexpat. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
