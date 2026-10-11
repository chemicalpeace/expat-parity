//! Prints the oracle trace of a file (or stdin) for debugging and for
//! regenerating golden traces: `oracle_trace [--ns] [--chunk N] [--encoding E] [FILE]`.
//! With `--golden`, rewrites `tests/vectors/upstream.traces.jsonl` from the
//! upstream vectors (default options).

use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut opts = expat_oracle::ParseOptions::default();
    let mut file = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--ns" => opts.ns_separator = Some(b'|'),
            "--chunk" => opts.chunk_size = args.next().ok_or("--chunk N")?.parse()?,
            "--encoding" => opts.encoding = Some(args.next().ok_or("--encoding E")?),
            "--golden" => return regen_golden(),
            _ => file = Some(a),
        }
    }
    let mut doc = Vec::new();
    match file {
        Some(f) => doc = std::fs::read(f)?,
        None => {
            std::io::stdin().read_to_end(&mut doc)?;
        }
    }
    let trace = expat_oracle::parse_trace(&doc, &opts)?;
    print!("{}", String::from_utf8_lossy(&trace));
    Ok(())
}

fn regen_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = String::new();
    for v in expat_difftest::vectors::upstream() {
        let trace = expat_oracle::parse_trace(&v.doc(), &expat_oracle::ParseOptions::default())?;
        let rec = serde_json::json!({ "id": v.id, "trace": String::from_utf8(trace)? });
        out.push_str(&serde_json::to_string(&rec)?);
        out.push('\n');
    }
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/upstream.traces.jsonl");
    std::fs::write(path, out)?;
    eprintln!("wrote {path}");
    Ok(())
}
