# Expat-Parity

A memory-safe Rust port of [libexpat](https://github.com/libexpat/libexpat)
2.9.0, the stream-oriented XML parser, verified by differential testing and
fuzzing against the vendored C reference. The crate is published as `expat`.

> **Status: `0.1.0-beta.1`, experimental.** This is an early-use MVP: the
> parser core (tokenizer, prolog, content, namespaces, DTD, internal
> entities, allocation limits) is ported and compared with libexpat, but the public
> API covers only part of libexpat's (see [Limitations](#limitations)), may
> change before 0.1.0, and has not been used in production. See
> [`docs/STATE.md`](https://github.com/chemicalpeace/expat-parity/blob/main/docs/STATE.md)
> for exactly what is done.

## Why

libexpat is embedded almost everywhere XML is parsed (CPython, browsers, OS
platforms) and has a long CVE record of memory-safety bugs. The aim is a port
whose behaviour is the same as libexpat's — same events, same errors, same
error positions — without memory-unsafety: `crates/expat` is
`#![forbid(unsafe_code)]`. How "the same" is defined and checked is described
in [`docs/ARCHITECTURE.md`](https://github.com/chemicalpeace/expat-parity/blob/main/docs/ARCHITECTURE.md).

## Usage

```toml
[dependencies]
expat = "=0.1.0-beta.1"
```

The parser is fed the document in one or more pieces; the last piece is
marked final. Events go to a `Handler`, whose `EVENTS` constant says which
events are reported:

```rust
use expat::{Events, Handler, Options, Parser};

#[derive(Default)]
struct Collect {
    elements: Vec<Vec<u8>>,
    text: Vec<u8>,
}

impl Handler for Collect {
    // Report only these two events (the default is `Events::ALL`).
    const EVENTS: Events = Events::NONE.start_element(true).character_data(true);

    fn start_element(&mut self, name: &[u8], _attributes: &[(&[u8], &[u8])], _specified: usize) {
        self.elements.push(name.to_vec());
    }

    fn character_data(&mut self, data: &[u8]) {
        self.text.extend_from_slice(data);
    }
}

fn main() -> Result<(), expat::Error> {
    // The salt keys the parser's hash tables. Use 16 bytes that whoever supplies
    // the document cannot predict (see "Hash salt" below); the fixed bytes here
    // are only for the example.
    let salt = [0x5a; 16];

    let mut parser = Parser::with_options(salt, Options::new().namespace_separator(b'|'));
    let mut collect = Collect::default();
    parser.parse_with(&mut collect, b"<doc xmlns='urn:x'><item>one</item>", false)?;
    parser.parse_with(&mut collect, b"<item>two</item></doc>", true)?;

    assert_eq!(collect.elements, [&b"urn:x|doc"[..], b"urn:x|item", b"urn:x|item"]);
    assert_eq!(collect.text, b"onetwo");

    // Errors carry libexpat's error code and the position where it occurred.
    let mut parser = Parser::new(salt);
    let error = parser.parse(b"<a></b>", true).unwrap_err();
    assert_eq!(error.code(), expat::ErrorCode::TagMismatch);
    assert_eq!(error.to_string(), "mismatched tag at line 1, column 5");
    Ok(())
}
```

`Parser::new(salt)` detects the encoding from the input and does no
namespace processing; `Options` sets an encoding name and a namespace
separator. `Parser::parse` parses without callbacks. Strings are passed as
UTF-8 `&[u8]`, whatever the document's encoding. The API documentation is on
[docs.rs](https://docs.rs/expat).

### Hash salt

libexpat seeds its hash tables from an entropy source. This crate is
`no_std` and has none, so the caller passes the 16-byte salt. If an attacker
who supplies the document can predict the salt (a constant, as in the
examples, or a guessable value), they can craft names that collide in the
parser's hash tables and make parsing much slower (hash flooding). Use bytes
from a cryptographic random source, for example the `getrandom` crate, for
documents you do not trust.

## Limitations

What `0.1.0-beta.1` does not do, or does differently from libexpat:

- **Callbacks:** only nine events — start and end element, character data,
  processing instruction, comment, CDATA section start and end, namespace
  declaration start and end. There is no default handler, no handler for
  DTD declarations, entity declarations, XML declarations or skipped
  entities, and no unknown-encoding handler (only libexpat's built-in
  encodings: UTF-8, UTF-16, UTF-16BE, UTF-16LE, ISO-8859-1, US-ASCII).
- **No position in callbacks, no stop or resume:** a handler cannot read the
  parser's position or stop the parse (`XML_StopParser` /
  `XML_ResumeParser`). The position is reported only with an error.
- **External entities and DTDs are never loaded.** The crate does no I/O, and
  the public API has no external entity handler: the parser behaves as
  libexpat does when no `XML_SetExternalEntityRefHandler` is set (compared
  with libexpat on its test vectors in this configuration). The internal
  DTD subset is parsed and internal entities are expanded.
- **Amplification limits** are libexpat 2.9.0's defaults and cannot be
  changed through the public API. The parser counts its allocations as
  libexpat does and refuses with `ErrorCode::NoMemory` or
  `ErrorCode::AmplificationLimitBreach` where libexpat does; this is
  checked in the differential tests, but not in production use.
- **Unfixed upstream security issues.** This crate follows libexpat 2.9.0,
  and libexpat 2.9.0 has security issues that upstream lists as unfixed and
  whose details are not public
  ([libexpat#1160](https://github.com/libexpat/libexpat/issues/1160)). One of
  them, CVE-2025-66382
  ([libexpat#1076](https://github.com/libexpat/libexpat/issues/1076)), is
  publicly described as denial of service: a crafted input of about 2 MiB
  can take tens of seconds to process. Whether this port is affected has not
  been verified, in either direction. The amplification limits count bytes,
  not time, so they do not guarantee bounded processing time. When parsing
  untrusted input, limit its size and the time spent parsing it in your
  application.
- **Real out-of-memory aborts.** When the system allocator itself fails,
  the crate does not return `ErrorCode::NoMemory` as libexpat does; Rust's
  allocation-error handler runs, which by default aborts the process.
- **Performance** has not been measured against libexpat.
- **MSRV:** Rust 1.99 (edition 2024).

These are deferred, with no commitment to implement them.

## Repository layout

| Path | What |
|---|---|
| `crates/expat` | the safe port, published as `expat` (`#![forbid(unsafe_code)]`, `no_std` + `alloc`) |
| `crates/expat-oracle` | vendored libexpat C at a pinned commit + FFI, **test-only**, not published |
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

MIT, same as libexpat, of which this is a derivative work. See
[`LICENSE`](https://github.com/chemicalpeace/expat-parity/blob/main/LICENSE) and
[`NOTICE`](https://github.com/chemicalpeace/expat-parity/blob/main/NOTICE);
both are included in the published crate.
