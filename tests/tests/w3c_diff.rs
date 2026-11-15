//! The W3C XML Conformance Test Suite (xmlts20130923) against the C
//! oracle: every test document of the catalogue, under several option
//! sets, compared by canonical trace.
//!
//! The suite is not vendored. Set `XMLCONF_DIR` to the extracted
//! `xmlconf` directory (`scripts/w3c-check.sh` downloads it, checks its
//! SHA-256 and runs this test); without it the test prints a note and
//! passes.
//!
//! External entities are read in memory through [`ExtHandler::Read`]:
//! before parsing, every `SYSTEM` literal in the document (and,
//! transitively, in the files it names) is resolved against the directory
//! of the file containing it, and the file found there is registered under
//! the literal. The library itself never fetches anything.
//!
//! The assertion is equality with libexpat. How libexpat (and therefore
//! the port) classifies the documents against the suite's expected result
//! is printed as a summary for docs/REPORT.md; it is not asserted, since
//! libexpat is a non-validating XML 1.0 parser and the suite also holds
//! XML 1.1, validity and namespace tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use expat_difftest::port::{port_trace, port_trace_fifth_edition_names};
use expat_difftest::trace::first_difference;
use expat_oracle::{ExtHandler, ParamEntityParsing, ParseOptions, parse_trace};

/// One `TEST` element of the catalogue.
struct Case {
    id: String,
    kind: String,
    path: PathBuf,
    /// The `EDITION` attribute: the XML 1.0 editions the expected result
    /// holds for, empty when it holds for all.
    edition: String,
}

/// Attribute `name` of the start tag `tag` (either quote style).
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(i) = rest.find(name) {
        let before_ok = i == 0 || rest.as_bytes()[i - 1].is_ascii_whitespace();
        let after = rest[i + name.len()..].trim_start();
        if before_ok && let Some(v) = after.strip_prefix('=') {
            let v = v.trim_start();
            let q = v.chars().next()?;
            if q == '"' || q == '\'' {
                let end = v[1..].find(q)?;
                return Some(v[1..=end].to_string());
            }
        }
        rest = &rest[i + name.len()..];
    }
    None
}

/// Every `TEST` element of the sub-catalogues named in `xmlconf.xml`.
fn catalogue(root: &Path) -> Vec<Case> {
    let main = std::fs::read_to_string(root.join("xmlconf.xml")).expect("xmlconf.xml");
    let mut out = Vec::new();
    for decl in main.split("<!ENTITY").skip(1) {
        let decl = &decl[..decl.find('>').unwrap_or(decl.len())];
        let Some(i) = decl.find("SYSTEM") else {
            continue;
        };
        let lit = decl[i + 6..].trim();
        let Some(sub) = lit.get(1..).and_then(|s| s.split(['"', '\'']).next()) else {
            continue;
        };
        let file = root.join(sub);
        let dir = file.parent().expect("dir").to_path_buf();
        let text = std::fs::read_to_string(&file).expect("sub-catalogue");
        for t in text.split("<TEST").skip(1) {
            if !t.starts_with(|c: char| c.is_ascii_whitespace()) {
                continue;
            }
            let tag = &t[..t.find('>').unwrap_or(t.len())];
            let (Some(id), Some(kind), Some(uri)) =
                (attr(tag, "ID"), attr(tag, "TYPE"), attr(tag, "URI"))
            else {
                panic!("TEST without ID, TYPE or URI in {sub}: {tag}");
            };
            out.push(Case {
                id,
                kind,
                path: dir.join(uri),
                edition: attr(tag, "EDITION").unwrap_or_default(),
            });
        }
    }
    out
}

/// The literals following `SYSTEM` in `text` (a lexical scan: a literal
/// inside a comment or a string is registered too, which only adds
/// entities nobody asks for).
fn system_literals(text: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = text[i..].windows(6).position(|w| w == b"SYSTEM") {
        let mut j = i + p + 6;
        while j < text.len() && text[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < text.len() && (text[j] == b'"' || text[j] == b'\'') {
            let q = text[j];
            if let Some(len) = text[j + 1..].iter().position(|&b| b == q) {
                out.push(text[j + 1..j + 1 + len].to_vec());
            }
        }
        i = j.max(i + p + 6);
    }
    // PUBLIC 'pub' 'sys': the system literal is the second one.
    let mut i = 0;
    while let Some(p) = text[i..].windows(6).position(|w| w == b"PUBLIC") {
        let mut j = i + p + 6;
        let mut lits = Vec::new();
        for _ in 0..2 {
            while j < text.len() && text[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < text.len() && (text[j] == b'"' || text[j] == b'\'') {
                let q = text[j];
                if let Some(len) = text[j + 1..].iter().position(|&b| b == q) {
                    lits.push(text[j + 1..j + 1 + len].to_vec());
                    j += len + 2;
                    continue;
                }
            }
            break;
        }
        if lits.len() == 2 {
            out.push(lits.swap_remove(1));
        }
        i = j.max(i + p + 6);
    }
    out
}

/// The external entities of `doc`: each `SYSTEM` literal found in the
/// document and, transitively, in the files it names, resolved against the
/// directory of the file that contains it; the first resolution of a
/// literal wins.
fn entities(doc: &Path, text: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut queue = vec![(doc.parent().expect("dir").to_path_buf(), text.to_vec())];
    while let Some((dir, body)) = queue.pop() {
        for lit in system_literals(&body) {
            if out.iter().any(|(k, _)| *k == lit) {
                continue;
            }
            let Ok(rel) = std::str::from_utf8(&lit) else {
                continue;
            };
            if rel.contains(':') {
                continue;
            }
            let file = dir.join(rel);
            if let Ok(bytes) = std::fs::read(&file) {
                let next = file.parent().expect("dir").to_path_buf();
                out.push((lit, bytes.clone()));
                queue.push((next, bytes));
            }
        }
    }
    out
}

fn result_code(trace: &[u8]) -> i64 {
    String::from_utf8_lossy(trace)
        .lines()
        .rev()
        .find(|l| l.starts_with("result "))
        .and_then(|l| l.split(' ').nth(2))
        .and_then(|c| c.parse().ok())
        .unwrap_or(-1)
}

/// The option sets: no external entities; every external entity read,
/// fed whole, in 1-byte chunks, through the buffer API in 7-byte chunks
/// and with namespaces.
fn option_sets(ents: &[(Vec<u8>, Vec<u8>)]) -> Vec<(&'static str, ParseOptions)> {
    let read = |chunk_size, buffer_api, ns_separator| ParseOptions {
        ext_handler: ExtHandler::Read,
        param_entity_parsing: ParamEntityParsing::Always,
        ext_entities: ents.to_vec(),
        not_standalone: Some(true),
        chunk_size,
        buffer_api,
        ns_separator,
        ..ParseOptions::default()
    };
    vec![
        ("plain", ParseOptions::default()),
        ("read", read(0, false, None)),
        ("read-1", read(1, false, None)),
        ("read-buf-7", read(7, true, None)),
        ("read-ns", read(0, false, Some(b'|'))),
    ]
}

#[test]
fn w3c_conformance_suite_matches_c() {
    let Some(root) = std::env::var_os("XMLCONF_DIR") else {
        eprintln!("XMLCONF_DIR not set: W3C suite skipped (scripts/w3c-check.sh runs it)");
        return;
    };
    let root = PathBuf::from(root);
    let cases = catalogue(&root);
    assert!(cases.len() > 2000, "catalogue: {} tests", cases.len());
    let mut failures = Vec::new();
    let mut runs = 0;
    let mut read_entities = 0;
    // (expected type, outcome of the "read" set) -> count
    let mut summary: BTreeMap<(String, &str), usize> = BTreeMap::new();
    for case in &cases {
        let doc = std::fs::read(&case.path)
            .unwrap_or_else(|e| panic!("{}: {}: {e}", case.id, case.path.display()));
        let ents = entities(&case.path, &doc);
        for (name, o) in option_sets(&ents) {
            let c = parse_trace(&doc, &o).expect("oracle");
            let t = port_trace(&doc, &o);
            runs += 1;
            if let Some((line, l, r)) = first_difference(&c, &t) {
                failures.push(format!(
                    "{} [{name}]: line {line}: C `{l}` port `{r}`",
                    case.id
                ));
            }
            if name == "read" {
                read_entities += String::from_utf8_lossy(&c)
                    .lines()
                    .filter(|l| l.starts_with("extend "))
                    .count();
                let outcome = if result_code(&c) == 0 {
                    "accepted"
                } else {
                    "rejected"
                };
                *summary.entry((case.kind.clone(), outcome)).or_default() += 1;
            }
        }
    }
    eprintln!(
        "W3C suite: {} tests, {runs} runs, {} mismatches; {read_entities} external entities read (\"read\" set)",
        cases.len(),
        failures.len()
    );
    for ((kind, outcome), n) in &summary {
        eprintln!("  {kind:8} {outcome:8} {n}");
    }
    for f in failures.iter().take(20) {
        eprintln!("{f}");
    }
    assert!(
        failures.is_empty(),
        "W3C suite: {} mismatches",
        failures.len()
    );
}

/// The suite with names by XML 1.0 Fifth Edition (D-064), an option of the
/// port that libexpat does not have. Each test document is parsed by the
/// C oracle and by the port with the option, under the "read" option set.
/// Asserted: wherever the results differ, the C library rejected the
/// document (the option only widens the names; the port either accepts it
/// or reports a later error), and no document whose expected result is "not-wf" for the Fifth
/// Edition becomes accepted. The differences are printed for
/// docs/REPORT.md.
#[test]
fn w3c_suite_with_fifth_edition_names() {
    let Some(root) = std::env::var_os("XMLCONF_DIR") else {
        eprintln!("XMLCONF_DIR not set: W3C suite skipped (scripts/w3c-check.sh runs it)");
        return;
    };
    let root = PathBuf::from(root);
    let cases = catalogue(&root);
    let mut problems = Vec::new();
    // (expected type, edition) -> number of documents now accepted
    let mut newly_accepted: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut other_error = 0;
    // Newly accepted documents whose expected result holds for every edition.
    let mut all_editions = Vec::new();
    for case in &cases {
        let doc = std::fs::read(&case.path)
            .unwrap_or_else(|e| panic!("{}: {}: {e}", case.id, case.path.display()));
        let ents = entities(&case.path, &doc);
        let Some((_, o)) = option_sets(&ents).into_iter().find(|(n, _)| *n == "read") else {
            panic!("no read option set");
        };
        let c = result_code(&parse_trace(&doc, &o).expect("oracle"));
        let t = result_code(&port_trace_fifth_edition_names(&doc, &o));
        if c == t {
            continue;
        }
        let fifth_applies = case.edition.is_empty() || case.edition.split(' ').any(|e| e == "5");
        if c == 0 {
            problems.push(format!("{}: C {c}, port {t}", case.id));
        } else if t != 0 {
            // Rejected by both, at a later token: the name is now
            // accepted and the error is found further on.
            other_error += 1;
        } else if case.kind == "not-wf" && fifth_applies {
            problems.push(format!(
                "{}: not-wf (edition {:?}) accepted",
                case.id, case.edition
            ));
        } else {
            *newly_accepted
                .entry((case.kind.clone(), case.edition.clone()))
                .or_default() += 1;
            if case.edition.is_empty() {
                all_editions.push(case.id.as_str());
            }
        }
    }
    let total: usize = newly_accepted.values().sum();
    eprintln!(
        "W3C suite, Fifth Edition names: {total} documents accepted that libexpat rejects; \
         {other_error} rejected by both with a different error"
    );
    for ((kind, edition), n) in &newly_accepted {
        eprintln!("  {kind:8} edition {edition:10} {n}");
    }
    eprintln!("  for every edition: {}", all_editions.join(" "));
    for p in problems.iter().take(20) {
        eprintln!("{p}");
    }
    assert!(problems.is_empty(), "{} problems", problems.len());
}
