//! Replays the committed corpora of the whole-parser fuzz targets
//! (`fuzz/corpus/parse_trace`, `fuzz/corpus/buffer_diff`) through the same
//! checks the fuzz targets run (`expat_difftest::fuzzcase`), so every CI
//! platform runs them, and pins the seed generator as deterministic.

use expat_difftest::fuzzcase::{buffer_diff_case, parse_trace_case, seeds};
use std::path::PathBuf;

fn corpus(target: &str) -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fuzz/corpus")
        .join(target);
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("corpus entry").path())
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let data = std::fs::read(&p).expect("corpus file");
            (p.file_name().unwrap().to_string_lossy().into_owned(), data)
        })
        .collect()
}

fn replay(target: &str, min: usize, check: fn(&[u8]) -> Result<(), String>) {
    let files = corpus(target);
    assert!(
        files.len() >= min,
        "{target}: {} corpus files, expected at least {min} (corpus missing?)",
        files.len()
    );
    let failures: Vec<String> = files
        .iter()
        .filter_map(|(name, data)| check(data).err().map(|e| format!("{name}: {e}")))
        .collect();
    eprintln!("{target}: {} corpus inputs replayed", files.len());
    for f in failures.iter().take(10) {
        eprintln!("{f}");
    }
    assert!(
        failures.is_empty(),
        "{} failing corpus inputs",
        failures.len()
    );
}

#[test]
fn parse_trace_corpus_matches_c() {
    replay("parse_trace", 5000, parse_trace_case);
}

#[test]
fn buffer_diff_corpus_matches_c() {
    replay("buffer_diff", 3000, buffer_diff_case);
}

#[test]
fn seed_generator_is_deterministic() {
    for t in ["parse_trace", "buffer_diff"] {
        let a = seeds(t).expect("seeds");
        let b = seeds(t).expect("seeds");
        assert_eq!(a, b, "{t}");
        assert!(a.len() >= 2540, "{t}: {} seeds", a.len());
    }
    assert!(seeds("leaf_diff").is_none());
}

#[test]
fn seeds_pass_their_checks() {
    for s in seeds("parse_trace").expect("seeds").iter().step_by(7) {
        parse_trace_case(s).unwrap();
    }
    for s in seeds("buffer_diff").expect("seeds").iter().step_by(7) {
        buffer_diff_case(s).unwrap();
    }
}
