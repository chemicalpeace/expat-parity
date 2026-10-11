//! Writes the deterministic seed inputs of the whole-parser fuzz targets
//! (`expat_difftest::fuzzcase::seeds`) to `<dir>/<target>/seed-<n>`:
//! `fuzz_seeds <dir> [target...]` (default: `parse_trace buffer_diff`).
//! See `docs/FUZZING.md` for how the committed corpora are regenerated.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage: fuzz_seeds <dir> [target...]")?);
    let mut targets: Vec<String> = args.collect();
    if targets.is_empty() {
        targets = vec!["parse_trace".into(), "buffer_diff".into()];
    }
    for t in &targets {
        let seeds =
            expat_difftest::fuzzcase::seeds(t).ok_or_else(|| format!("no seeds for {t}"))?;
        let out = dir.join(t);
        std::fs::create_dir_all(&out)?;
        for (i, s) in seeds.iter().enumerate() {
            std::fs::write(out.join(format!("seed-{i:05}")), s)?;
        }
        println!("{t}: {} seeds in {}", seeds.len(), out.display());
    }
    Ok(())
}
