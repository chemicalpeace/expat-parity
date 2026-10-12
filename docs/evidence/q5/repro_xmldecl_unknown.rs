//! Port-vs-C reproduction of the Q-5 position difference (tok level; the port has no unknown-encoding handler in its Parser yet). Run: copy to tests/examples/ and `cargo run -p expat-difftest --example repro_xmldecl_unknown`.
use expat::internal::tok as rs;
use expat_difftest::tok::{Rng, random_map, unknown_from_seed};
use expat_oracle::tok as c;
fn main() {
    {
        let p = unknown_from_seed(0x52f43d23123766f6).unwrap();
        let more = b"<?xml \xC3\xA9?><";
        for i in 6..8 {
            println!("more to_ascii {i}: {}", rs::to_ascii(&p.rs, more, i, 8));
        }
        let r = rs::parse_xml_decl(false, &p.rs, false, more, 0, 10);
        println!("{:?}", r.bad);
    }
    for (seed, doc) in [
        (0x52f43d23123766f6u64, &b"<?xml \xC3\xA9?><a/>"[..]),
        (0xfe8d9b29cbd0ff22, &b"<?xml no\xC3\xA9 ?><a/>"[..]),
    ] {
        let mut r = Rng::new(seed);
        let map = random_map(&mut r);
        let p = unknown_from_seed(seed).expect("valid map");
        let end = doc.iter().position(|&b| b == b'>').unwrap() + 1;
        println!(
            "seed {seed:#x} doc {:?} decl end {end}",
            String::from_utf8_lossy(doc)
        );
        for i in 6..end {
            println!(
                "  byte {i} {:#04x} map {}",
                doc[i],
                map[usize::from(doc[i])]
            );
        }
        for ns in [false, true] {
            for gte in [false, true] {
                let rr = rs::parse_xml_decl(gte, &p.rs, ns, doc, 0, end);
                let cc = c::parse_xml_decl(ns, gte, c::Enc::Unknown(&p.c), doc, 0, end);
                println!(
                    "  ns={ns} gte={gte}: port ok={} bad={:?} | C ok={} bad={:?}",
                    rr.ok, rr.bad, cc.ok, cc.bad
                );
            }
        }
        for i in 6..end - 2 {
            println!(
                "  to_ascii({i}, end {}) port {} ",
                end - 2,
                rs::to_ascii(&p.rs, doc, i, end - 2)
            );
        }
    }
}
