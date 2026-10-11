//! Module 10: white-box differential test of the input buffer
//! (`XML_Parse`, `XML_GetBuffer`, `XML_ParseBuffer`, reparse deferral).
//!
//! The oracle (`oracle_buffer_run` in `shim/oracle_core.c`) and the port
//! run the same operation sequence on the same document; after every
//! operation the status, the error code and the buffer state (`m_bufferPtr`,
//! `m_bufferEnd`, `m_bufferLim` as offsets, `m_partialTokenBytesBefore`,
//! `m_lastBufferRequestSize`, `m_parseEndByteIndex`, `m_eventPtr`,
//! `m_positionPtr`, the pointer `XML_GetBuffer` returned) must be equal, and
//! so must the final line, column and byte index.

use expat_difftest::docgen::{Rng, document};
use expat_difftest::port::buffer_run;
use expat_oracle::BufferOp;

fn check(doc: &[u8], ops: &[BufferOp], ns: bool, deferral: bool) {
    let c = expat_oracle::buffer_run(doc, ops, ns, deferral);
    let r = buffer_run(doc, ops, ns, deferral);
    assert_eq!(
        r,
        c,
        "ops {ops:?} ns {ns} deferral {deferral}\ndoc {:?}",
        String::from_utf8_lossy(doc)
    );
}

/// Requests that are either at least `n` and small enough to allocate, or
/// fail without allocating (negative, or so large that C's size
/// computation overflows). `XML_ParseBuffer` accepts any length up to the
/// end of the allocated buffer, but an application may only rely on the
/// `len` it asked `XML_GetBuffer` for; parsing bytes it never wrote would
/// compare uninitialised C memory, so the tests keep `n <= req`.
fn random_req(r: &mut Rng, n: i32) -> i32 {
    match r.below(10) {
        0 => -1 - r.below(3) as i32,
        1 => i32::MAX - r.below(1000) as i32,
        2 => n,
        3 => n.max(r.below(70_000) as i32),
        _ => n + r.below(3000) as i32,
    }
}

fn random_ops(r: &mut Rng, len: usize) -> Vec<BufferOp> {
    let mut ops = Vec::new();
    let mut left = len as i64;
    let steps = 1 + r.below(12);
    for i in 0..steps {
        let last = i + 1 == steps;
        let n = if last {
            left.max(0) as i32
        } else {
            match r.below(5) {
                0 => 1,
                1 => r.below(8) as i32,
                2 => r.below(1100) as i32,
                3 => r.below(5000) as i32,
                _ => (left / 2).max(0) as i32,
            }
        };
        let is_final = last || r.one_in(15);
        if r.one_in(2) {
            ops.push(BufferOp::Parse { n, is_final });
        } else {
            let req = random_req(r, n);
            ops.push(BufferOp::Buffer { req, n, is_final });
        }
        left -= i64::from(n);
    }
    // Sometimes call again after the end (finished / error states).
    if r.one_in(5) {
        ops.push(BufferOp::Buffer {
            req: 10,
            n: 0,
            is_final: true,
        });
        ops.push(BufferOp::Parse {
            n: 0,
            is_final: true,
        });
    }
    ops
}

#[test]
fn buffer_management_matches_c_on_generated_sequences() {
    let mut r = Rng::new(0x6275_6666_6572);
    for _ in 0..3000 {
        let doc = document(&mut r);
        let ops = random_ops(&mut r, doc.len());
        let ns = r.one_in(3);
        let deferral = !r.one_in(4);
        check(&doc, &ops, ns, deferral);
    }
}

#[test]
fn buffer_edge_cases_match_c() {
    let doc = b"<doc a='1'>text<![CDATA[x]]></doc>\r\n";
    let small = |n| BufferOp::Parse { n, is_final: false };
    let cases: Vec<Vec<BufferOp>> = vec![
        // XML_ParseBuffer before any XML_GetBuffer: XML_ERROR_NO_BUFFER.
        vec![
            BufferOp::Buffer {
                req: -1,
                n: 0,
                is_final: false,
            },
            BufferOp::Parse {
                n: 40,
                is_final: true,
            },
        ],
        // len larger than the free space of the buffer: XML_ERROR_INVALID_ARGUMENT.
        vec![BufferOp::Buffer { req: 5, n: 5000, is_final: false }, BufferOp::Parse { n: 40, is_final: true }],
        // Empty requests and parses.
        vec![
            BufferOp::Buffer {
                req: 0,
                n: 0,
                is_final: false,
            },
            BufferOp::Parse {
                n: 0,
                is_final: false,
            },
            BufferOp::Parse {
                n: 40,
                is_final: true,
            },
        ],
        // Overflowing requests.
        vec![
            BufferOp::Buffer {
                req: i32::MAX,
                n: 0,
                is_final: false,
            },
            BufferOp::Parse {
                n: 40,
                is_final: true,
            },
        ],
        vec![
            small(3),
            BufferOp::Buffer {
                req: i32::MAX - 2,
                n: 0,
                is_final: false,
            },
        ],
        // Byte by byte, then everything after the end.
        (0..doc.len() as i32)
            .map(|_| small(1))
            .chain([
                BufferOp::Parse {
                    n: 0,
                    is_final: true,
                },
                BufferOp::Parse {
                    n: 0,
                    is_final: true,
                },
                BufferOp::Buffer {
                    req: 4,
                    n: 0,
                    is_final: false,
                },
            ])
            .collect(),
    ];
    for ops in &cases {
        for ns in [false, true] {
            for deferral in [true, false] {
                check(doc, ops, ns, deferral);
            }
        }
    }
}

#[test]
fn large_documents_cross_buffer_boundaries_like_c() {
    // A document much longer than the 1024 context bytes, fed in pieces
    // around the buffer sizes, with tokens that span the pieces.
    let mut doc = b"<doc>".to_vec();
    for i in 0..400 {
        doc.extend_from_slice(format!("<e a='{i}'>{}</e>\r\n", "x".repeat(i % 97)).as_bytes());
    }
    doc.extend_from_slice(b"</doc>");
    for piece in [1usize, 7, 511, 1023, 1024, 1025, 2047, 4096] {
        for deferral in [true, false] {
            let mut ops: Vec<BufferOp> = doc
                .chunks(piece)
                .map(|c| BufferOp::Parse {
                    n: c.len() as i32,
                    is_final: false,
                })
                .collect();
            ops.push(BufferOp::Parse {
                n: 0,
                is_final: true,
            });
            check(&doc, &ops, false, deferral);
            let ops: Vec<BufferOp> = doc
                .chunks(piece)
                .enumerate()
                .map(|(i, c)| BufferOp::Buffer {
                    req: (c.len() + i % 3 * 700) as i32,
                    n: c.len() as i32,
                    is_final: false,
                })
                .chain([BufferOp::Buffer {
                    req: 0,
                    n: 0,
                    is_final: true,
                }])
                .collect();
            check(&doc, &ops, true, deferral);
        }
    }
}
