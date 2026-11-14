# Throughput: port, C libexpat, C libexpat built with Fil-C

Throughput harness that parses the same documents with the port, with C
libexpat 2.10.0 built by a plain C compiler, and with the same C sources
built by Fil-C (a memory-safe C implementation).

Three builds parse the same in-memory documents:

- **port**: `benches/src/bin/xmlbench.rs` on the `expat` crate, `--release`
  (the workspace profile) and `--release` with `lto = "fat"`,
  `codegen-units = 1` (set through `CARGO_PROFILE_RELEASE_*`, the workspace
  profile is unchanged);
- **C**: `xmlbench.c` linked with the vendored libexpat 2.10.0 sources of
  `crates/expat-oracle/vendor` (unmodified, same `shim/expat_config.h` as the
  oracle, so the same configuration as the port), `-O2 -DNDEBUG`, with clang
  and with gcc;
- **Fil-C**: the same C sources and flags compiled with the Fil-C `clang` of a
  prebuilt release (<https://github.com/pizlonator/fil-c/releases>).

Workload: one parser per run, the whole document from memory through
`XML_Parse` / `Parser::parse`, (1) without handlers and (2) with start/end
element and character data handlers counting events, each as a single call
and as 64 KiB calls. One untimed warm-up run, then `RUNS` (default 7) timed
runs; the median is reported in MiB/s of input bytes, pinned to one core with
`taskset`. Every harness prints its event counts; `table.py` fails if they
differ.

```sh
FILC=/path/to/filc-0.687-linux-aarch64 benches/filc/run.sh build
benches/filc/run.sh corpus          # synthetic documents (gen_corpus.py)
# optional real-world file, e.g. an OpenStreetMap API extract (ODbL):
curl -o /tmp/expat-bench/corpus/osm-istanbul.osm \
  'https://api.openstreetmap.org/api/0.6/map?bbox=28.970,41.005,28.985,41.015'
benches/filc/run.sh run             # appends to /tmp/expat-bench/results.txt
python3 benches/filc/table.py /tmp/expat-bench/results.txt
```

Documents whose file name starts with `ns-` are parsed with namespace
processing (separator `|`). The corpus is generated, not committed. Results
are in `docs/REPORT.md`, "Benchmarks".
