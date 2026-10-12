//! Prints the C and port traces of a document given on the command line.
fn main() {
    let doc = std::env::args().nth(1).unwrap_or_default().into_bytes();
    let chunk: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let ns = std::env::args().nth(3).is_some();
    let o = expat_oracle::ParseOptions {
        chunk_size: chunk,
        ns_separator: ns.then_some(b'|'),
        ..Default::default()
    };
    let c = expat_oracle::parse_trace(&doc, &o).expect("oracle");
    println!("C:\n{}", String::from_utf8_lossy(&c));
    let t = expat_difftest::port::port_trace(&doc, &o);
    println!("port:\n{}", String::from_utf8_lossy(&t));
}
