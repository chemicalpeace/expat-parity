# DV-1 evidence (scratch branch, not for main)

- `tests/tests/dv1_evidence.rs`: measurements (surplus per document, default-limit
  windows in both directions, pool-slack boundary, binding / default-attribute
  scaling). Run: `cargo test --release -p expat-difftest --test dv1_evidence -- --nocapture --test-threads=1`
  with `DV1_NDEF=0|10000` (window_both_sides), `DV1_SLACK=84947:360:370 [DV1_MB=1]`
  (pool_slack_boundary), `EXPAT_MALLOC_DEBUG=2 DV1_DOC=...` (c_alloc_trace).
- `c_alloc_driver.c`: standalone libexpat 2.9.0 driver; with `EXPAT_MALLOC_DEBUG=2`
  libexpat prints every tracked allocation with its xmlparse.c line.
  Build: `cc -DXML_STATIC -Icrates/expat-oracle/shim -I$L dv1/c_alloc_driver.c $L/xmlparse.c $L/xmlrole.c $L/xmltok.c $L/xcs.c $L/random_dev_urandom.c`
  (`L=crates/expat-oracle/vendor/expat/lib`); usage `./a.out doc.xml <0|1|2 ns> [chunk]`.
- `meas-v3-NOT-APPLIED.patch`: the local measurement variant (C sizes for BINDING,
  NAME_AND_DEFAULT_ATTRIBUTE, ELEMENT_TYPE; CREATION_CHARGE 3040; POOL_SLACK 0;
  negative end margins printed instead of failing). Measurement only: it breaks
  the >= C relation (889 negative end margins, 889 relation failures at tight limits).
