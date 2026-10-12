# Q-5 evidence (docs/DIVERGENCES.md Q-5, D-035)

| File | What |
|---|---|
| `repro_xmldecl_unknown.rs`, `repro-port-before.txt`, `repro-port-after.txt` | The two inputs of the random test on the port and the C tokenizer (`XmlParseXmlDecl` with the seed's unknown encoding): `bad` 7/8 and 9/11 before the fix, equal after. |
| `q5_overread.c`, `build.sh`, `run.sh` | C program against the vendored libexpat 2.9.0 (public API: `XML_ParserCreate("x-q5")` + unknown-encoding handler, `XML_Parse` and `XML_GetBuffer`/`XML_ParseBuffer`), built plain, with ASan and with MSan (clang). |
| `plain.txt` | Converter call offsets: C calls the converter on bytes after the document (`<-- past the document`). |
| `asan.txt` | `AddressSanitizer: heap-buffer-overflow` READ in `unknown_toUtf8` (`xmltok.c` 1283) under `toAscii` / `parsePseudoAttribute` / `XmlParseXmlDecl` / `processXmlDecl`, past the parser's input buffer allocated by `XML_GetBuffer` (`xmlparse.c` 2603), for both seed documents ending at `?>`. |
| `msan.txt` | With a map that only has the two seed lead bytes: `MemorySanitizer: use-of-uninitialized-value` in `unknown_toUtf8` (`xmltok.c` 1284), uninitialised heap from `XML_GetBuffer`. |
| `miri.txt` | The port's bounds tests (`tok::tests::q5`) under Miri. |
| `mutation-check.txt` | The C-differential regression tests fail with the fix reverted. |
| `random-scan.txt` | Long runs of `init_diff::xml_decl_in_every_builtin_encoding`. |

Security report: `docs/security/Q-5-libexpat-2.9.0-unknown_toUtf8-overread.md`. Re-run on main a990a8c: `rerun-main-a990a8c/`. `check.sh` checks the outputs (CI job `q5-sanitizers`).

Reproduce: `docs/evidence/q5/build.sh && docs/evidence/q5/run.sh && docs/evidence/q5/check.sh` (clang with ASan and MSan runtimes; `OUT` = build directory, default `/tmp/q5`).
