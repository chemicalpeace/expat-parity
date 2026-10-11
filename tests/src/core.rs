//! Differential checks of the string pool and hash table (module 9) against
//! `xmlparse.c`, white-box.
//!
//! A *program* is a list of pool operations ([`PoolOp`]) or hash table
//! operations ([`HashOp`]). [`run_c`] encodes it and runs it in
//! `shim/oracle_core.c` on the real C structures; [`run_port`] runs it on
//! `expat::internal::pool` / `hash`. Both write the same text trace, which
//! records after every operation its result, every allocation request
//! (with its size and whether the harness heap granted it), and the complete
//! white-box state:
//!
//! - pool: block sizes of both lists, `start`/`ptr` and the bytes of the
//!   current string, and a digest of every finished string still alive;
//! - hash table: `power`, `size`, `used`, the result's entry id and slot,
//!   the slot order at every `Iterate`, and the full slot layout at the end.
//!
//! Program encoding (little-endian `u32` lengths): header byte (0 pool,
//! 1 hash table), 16 salt bytes (`XML_SetHashSalt16Bytes`), then per
//! operation an opcode byte and its arguments, in the order of the enum
//! variants below.
//!
//! The harness heap ([`HarnessHeap`], mirrored by the shim's allocation
//! hooks) refuses the attempt chosen by `FailNext` and anything beyond
//! [`expat_oracle::CORE_HEAP_CAP`] bytes in total; libexpat's own allocation
//! tracker is disabled in the shim.

use std::fmt::Write as _;

use expat::internal::hash::HashTable;
use expat::internal::heap::Heap;
use expat::internal::pool::{PoolStr, StringPool};
use expat::internal::siphash::SipKey;
use expat::internal::tok::{Encoding, Normal, Utf16};

/// A string pool operation (`xmlparse.c` function or macro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolOp {
    /// `poolAppendChar`.
    AppendChar(u8),
    /// `poolAppendChars` (may contain NUL).
    AppendChars(Vec<u8>),
    /// `poolAppend` from built-in encoding `enc % 12` (`Builtin::ALL` order).
    Append(u8, Vec<u8>),
    /// `poolStoreString`.
    StoreString(u8, Vec<u8>),
    /// `poolCopyString` (no NUL).
    CopyString(Vec<u8>),
    /// `poolCopyStringNoFinish` (no NUL).
    CopyStringNoFinish(Vec<u8>),
    /// `poolCopyStringN`.
    CopyStringN(Vec<u8>),
    /// `poolAppendString` (no NUL).
    AppendString(Vec<u8>),
    /// `poolGrow`.
    Grow,
    /// `poolGrowUntil`.
    GrowUntil(u32),
    /// `poolFinish`.
    Finish,
    /// `poolDiscard`.
    Discard,
    /// `poolChop` if the current string is not empty.
    Chop,
    /// `poolClear`.
    Clear,
    /// The `k`-th next allocation attempt fails.
    FailNext(u32),
    /// `poolLastChar` if the current string is not empty.
    LastChar,
}

/// A hash table operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashOp {
    /// `lookup(parser, table, key, createSize)` (no NUL in the key;
    /// `createSize` is raised to the harness entry size).
    Insert(Vec<u8>, u32),
    /// `lookupWithLength(parser, table, key, len, 0)` (no NUL).
    Find(Vec<u8>),
    /// `hashTableClear`.
    Clear,
    /// `hashTableIterInit` / `hashTableIterNext` over the whole table.
    Iterate,
    /// The `k`-th next allocation attempt fails.
    FailNext(u32),
}

fn put_str(out: &mut Vec<u8>, s: &[u8]) {
    let n = u32::try_from(s.len()).expect("string fits u32");
    out.extend(n.to_le_bytes());
    out.extend(s);
}

/// Encodes a pool program.
pub fn encode_pool(ops: &[PoolOp]) -> Vec<u8> {
    let mut out = vec![0u8; 17];
    for op in ops {
        match op {
            PoolOp::AppendChar(c) => out.extend([0, *c]),
            PoolOp::AppendChars(s) => {
                out.push(1);
                put_str(&mut out, s);
            }
            PoolOp::Append(e, s) | PoolOp::StoreString(e, s) => {
                out.push(if matches!(op, PoolOp::Append(..)) {
                    2
                } else {
                    3
                });
                out.push(*e);
                put_str(&mut out, s);
            }
            PoolOp::CopyString(s)
            | PoolOp::CopyStringNoFinish(s)
            | PoolOp::CopyStringN(s)
            | PoolOp::AppendString(s) => {
                let code = match op {
                    PoolOp::CopyString(_) => 4,
                    PoolOp::CopyStringNoFinish(_) => 5,
                    PoolOp::CopyStringN(_) => 6,
                    _ => 7,
                };
                assert!(code == 6 || !s.contains(&0), "C string op without NUL");
                out.push(code);
                put_str(&mut out, s);
            }
            PoolOp::Grow => out.push(8),
            PoolOp::GrowUntil(n) => {
                out.push(9);
                out.extend(n.to_le_bytes());
            }
            PoolOp::Finish => out.push(10),
            PoolOp::Discard => out.push(11),
            PoolOp::Chop => out.push(12),
            PoolOp::Clear => out.push(13),
            PoolOp::FailNext(k) => {
                out.push(14);
                out.extend(k.to_le_bytes());
            }
            PoolOp::LastChar => out.push(15),
        }
    }
    out
}

/// Encodes a hash table program with `salt`.
pub fn encode_hash(salt: [u8; 16], ops: &[HashOp]) -> Vec<u8> {
    let mut out = vec![1u8];
    out.extend(salt);
    for op in ops {
        match op {
            HashOp::Insert(k, size) => {
                assert!(!k.contains(&0), "key without NUL");
                out.push(0);
                put_str(&mut out, k);
                out.extend(size.to_le_bytes());
            }
            HashOp::Find(k) => {
                assert!(!k.contains(&0), "key without NUL");
                out.push(1);
                put_str(&mut out, k);
            }
            HashOp::Clear => out.push(2),
            HashOp::Iterate => out.push(3),
            HashOp::FailNext(k) => {
                out.push(4);
                out.extend(k.to_le_bytes());
            }
        }
    }
    out
}

/// Runs an encoded program on the C oracle.
pub fn run_c(program: &[u8]) -> String {
    expat_oracle::core_run(program).expect("well-formed program")
}

/// The allocation rule of the harness (see the module documentation),
/// writing the shim's allocation lines into `out`.
pub struct HarnessHeap {
    /// The trace being written.
    pub out: String,
    attempts: u64,
    fail_at: u64,
    total: usize,
}

impl HarnessHeap {
    fn new() -> Self {
        Self {
            out: String::new(),
            attempts: 0,
            fail_at: u64::MAX,
            total: 0,
        }
    }

    fn decide(&mut self, increase: usize) -> bool {
        let ok =
            self.attempts != self.fail_at && increase <= expat_oracle::CORE_HEAP_CAP - self.total;
        self.attempts += 1;
        ok
    }

    fn fail_next(&mut self, k: u32) {
        self.fail_at = self.attempts + u64::from(k);
    }
}

fn verdict(ok: bool) -> &'static str {
    if ok { "ok" } else { "fail" }
}

impl Heap for HarnessHeap {
    fn alloc(&mut self, size: usize) -> bool {
        let ok = self.decide(size);
        writeln!(self.out, "  alloc {size} {}", verdict(ok)).expect("String");
        if ok {
            self.total += size;
        }
        ok
    }

    fn realloc(&mut self, old: usize, size: usize) -> bool {
        let ok = self.decide(size.saturating_sub(old));
        writeln!(self.out, "  realloc {old} {size} {}", verdict(ok)).expect("String");
        if ok {
            self.total = self.total - old + size;
        }
        ok
    }

    fn free(&mut self, size: usize) {
        writeln!(self.out, "  free {size}").expect("String");
        self.total -= size;
    }
}

fn hex(out: &mut String, bytes: &[u8]) {
    for b in bytes {
        write!(out, "{b:02x}").expect("String");
    }
}

const FNV_INIT: u64 = 0xcbf2_9ce4_8422_2325;

fn fnv(mut h: u64, data: &[u8]) -> u64 {
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// The current string: its bytes in hex up to 64 bytes, otherwise its
/// length and digest.
fn current_summary(out: &mut String, cur: &[u8]) {
    if cur.len() <= 64 {
        hex(out, cur);
    } else {
        write!(out, "#{}:{:016x}", cur.len(), fnv(FNV_INIT, cur)).expect("String");
    }
}

fn sizes(v: &[i32]) -> String {
    let parts: Vec<String> = v.iter().map(i32::to_string).collect();
    format!("[{}]", parts.join(","))
}

/// `poolAppend` or `poolStoreString` from built-in encoding `e % 12`.
fn append_builtin(
    pool: &mut StringPool,
    heap: &mut HarnessHeap,
    e: u8,
    s: &[u8],
    store: bool,
) -> Option<PoolStr> {
    fn go<E: Encoding>(
        pool: &mut StringPool,
        heap: &mut HarnessHeap,
        enc: &E,
        s: &[u8],
        store: bool,
    ) -> Option<PoolStr> {
        if store {
            pool.store_string(heap, enc, s).ok()
        } else {
            pool.append(heap, enc, s).ok()
        }
    }
    let i = usize::from(e % 12);
    match Normal::ALL.get(i) {
        Some(n) => go(pool, heap, n, s, store),
        None => go(pool, heap, &Utf16::ALL[i - 8], s, store),
    }
}

/// Runs a pool program on the port.
pub fn run_port_pool(ops: &[PoolOp]) -> String {
    let mut heap = HarnessHeap::new();
    let mut pool = StringPool::new();
    let mut regions: Vec<PoolStr> = Vec::new();
    for (idx, op) in ops.iter().enumerate() {
        let o = &mut heap.out;
        let mut finished: Option<PoolStr> = None;
        // `None`: the operation reports ok/fail; `Some`: a string or fail.
        let mut ret: Option<Option<PoolStr>> = None;
        let mut ok = true;
        match op {
            PoolOp::AppendChar(c) => {
                writeln!(o, "{idx} append_char {c:02x}").expect("String");
                ok = pool.append_char(&mut heap, *c).is_ok();
            }
            PoolOp::AppendChars(s) => {
                writeln!(o, "{idx} append_chars {}", s.len()).expect("String");
                ok = pool.append_chars(&mut heap, s).is_ok();
            }
            PoolOp::Append(e, s) | PoolOp::StoreString(e, s) => {
                let store = matches!(op, PoolOp::StoreString(..));
                let name = if store { "store_string" } else { "append" };
                writeln!(o, "{idx} {name} {} {}", e % 12, s.len()).expect("String");
                ret = Some(append_builtin(&mut pool, &mut heap, *e, s, store));
            }
            PoolOp::CopyString(s) => {
                writeln!(o, "{idx} copy_string {}", s.len()).expect("String");
                let r = pool.copy_string(&mut heap, s).ok();
                finished = r;
                ret = Some(r);
            }
            PoolOp::CopyStringNoFinish(s) => {
                writeln!(o, "{idx} copy_string_no_finish {}", s.len()).expect("String");
                ret = Some(pool.copy_string_no_finish(&mut heap, s).ok());
            }
            PoolOp::CopyStringN(s) => {
                writeln!(o, "{idx} copy_string_n {}", s.len()).expect("String");
                let r = pool.copy_string_n(&mut heap, s).ok();
                finished = r;
                ret = Some(r);
            }
            PoolOp::AppendString(s) => {
                writeln!(o, "{idx} append_string {}", s.len()).expect("String");
                ret = Some(pool.append_string(&mut heap, s).ok());
            }
            PoolOp::Grow => {
                writeln!(o, "{idx} grow").expect("String");
                ok = pool.grow(&mut heap).is_ok();
            }
            PoolOp::GrowUntil(n) => {
                writeln!(o, "{idx} grow_until {n}").expect("String");
                ok = pool.grow_until(&mut heap, *n as usize).is_ok();
            }
            PoolOp::Finish => {
                writeln!(o, "{idx} finish").expect("String");
                finished = Some(pool.start());
                pool.finish();
            }
            PoolOp::Discard => {
                writeln!(o, "{idx} discard").expect("String");
                pool.discard();
            }
            PoolOp::Chop => {
                writeln!(o, "{idx} chop").expect("String");
                if !pool.is_empty() {
                    pool.chop();
                }
            }
            PoolOp::Clear => {
                writeln!(o, "{idx} clear").expect("String");
                pool.clear();
                regions.clear();
            }
            PoolOp::FailNext(k) => {
                writeln!(o, "{idx} fail_next {k}").expect("String");
                heap.fail_next(*k);
            }
            PoolOp::LastChar => match pool.last_char() {
                Some(c) => writeln!(o, "{idx} last_char {c:02x}").expect("String"),
                None => writeln!(o, "{idx} last_char none").expect("String"),
            },
        }
        if let Some(r) = finished
            && r.len > 0
        {
            regions.push(r);
        }
        let o = &mut heap.out;
        match ret {
            Some(Some(s)) => writeln!(o, "  -> str {} {}", s.block, s.start),
            Some(None) => writeln!(o, "  -> fail"),
            None => writeln!(o, "  -> {}", verdict(ok)),
        }
        .expect("String");
        let l = pool.layout();
        write!(
            o,
            "  blocks {} free {}",
            sizes(&l.blocks),
            sizes(&l.free_blocks)
        )
        .expect("String");
        match l.cursor {
            Some((start, ptr)) => {
                write!(o, " cur {start} {ptr} ").expect("String");
                current_summary(o, pool.current());
            }
            None => o.push_str(" cur null"),
        }
        let mut h = FNV_INIT;
        for r in &regions {
            h = fnv(h, &r.len.to_ne_bytes());
            h = fnv(h, pool.get(*r).expect("finished string is alive"));
        }
        writeln!(o, " regions {} {h:016x}", regions.len()).expect("String");
    }
    heap.out.push_str("destroy\n");
    pool.destroy(&mut heap);
    heap.out
}

/// `sizeof(ENTRY)` in the shim: a `NAMED` and a `size_t` id.
const ENTRY_SIZE: u32 = 2 * std::mem::size_of::<usize>() as u32;

/// Runs a hash table program on the port.
pub fn run_port_hash(salt: [u8; 16], ops: &[HashOp]) -> String {
    let key = SipKey::from_bytes(salt);
    let mut heap = HarnessHeap::new();
    let mut table: HashTable<()> = HashTable::new();
    let slot_of = |t: &HashTable<()>, k: &[u8]| t.find_slot(&key, k).map_or(-1, |p| p as i64);
    for (idx, op) in ops.iter().enumerate() {
        match op {
            HashOp::Insert(k, size) => {
                let create = (*size).max(ENTRY_SIZE);
                write!(heap.out, "{idx} insert {} ", k.len()).expect("String");
                hex(&mut heap.out, k);
                writeln!(heap.out, " {create}").expect("String");
                let r = table.lookup_or_insert(&key, &mut heap, k, create as usize, || ());
                let line = match r {
                    Err(_) => "  -> fail".to_string(),
                    Ok((id, l)) => {
                        let what = match l {
                            expat::internal::hash::Lookup::Found => "found",
                            expat::internal::hash::Lookup::Inserted => "inserted",
                        };
                        format!("  -> {what} {} {}", id.0, slot_of(&table, k))
                    }
                };
                writeln!(heap.out, "{line}").expect("String");
            }
            HashOp::Find(k) => {
                write!(heap.out, "{idx} find {} ", k.len()).expect("String");
                hex(&mut heap.out, k);
                heap.out.push('\n');
                match table.find(&key, k) {
                    Some(id) => {
                        writeln!(heap.out, "  -> found {} {}", id.0, slot_of(&table, k))
                    }
                    None => writeln!(heap.out, "  -> none"),
                }
                .expect("String");
            }
            HashOp::Clear => {
                writeln!(heap.out, "{idx} clear").expect("String");
                table.clear(&mut heap);
            }
            HashOp::Iterate => {
                writeln!(heap.out, "{idx} iterate").expect("String");
                heap.out.push_str("  ->");
                for id in table.iter() {
                    write!(heap.out, " {}", id.0).expect("String");
                }
                heap.out.push('\n');
            }
            HashOp::FailNext(k) => {
                writeln!(heap.out, "{idx} fail_next {k}").expect("String");
                heap.fail_next(*k);
            }
        }
        writeln!(
            heap.out,
            "  power {} size {} used {}",
            table.power(),
            table.size(),
            table.len()
        )
        .expect("String");
    }
    heap.out.push_str("slots");
    for s in table.slots() {
        match s {
            Some(id) => write!(heap.out, " {}", id.0).expect("String"),
            None => heap.out.push_str(" -"),
        }
    }
    heap.out.push_str("\ndestroy\n");
    table.destroy(&mut heap);
    heap.out
}

/// Panics with the first differing line if the port and C disagree on a
/// pool program.
pub fn check_pool(ops: &[PoolOp]) {
    let c = run_c(&encode_pool(ops));
    let r = run_port_pool(ops);
    assert_same(&c, &r, &format!("pool program {ops:?}"));
}

/// Panics with the first differing line if the port and C disagree on a
/// hash table program.
pub fn check_hash(salt: [u8; 16], ops: &[HashOp]) {
    let c = run_c(&encode_hash(salt, ops));
    let r = run_port_hash(salt, ops);
    assert_same(&c, &r, &format!("hash program, salt {salt:02x?}"));
}

fn assert_same(c: &str, r: &str, what: &str) {
    if c == r {
        return;
    }
    let line = c
        .lines()
        .zip(r.lines())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| c.lines().count().min(r.lines().count()));
    let ctx = |t: &str| -> String {
        t.lines()
            .skip(line.saturating_sub(6))
            .take(8)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut what = what.to_string();
    what.truncate(2000);
    panic!(
        "port differs from C at trace line {line}\n--- C\n{}\n--- port\n{}\n--- {what}",
        ctx(c),
        ctx(r)
    );
}

/// Reads fuzz-style bytes as operation arguments.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn u8(&mut self) -> Option<u8> {
        let (&b, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(b)
    }

    /// A length: low 5 bits, shifted left by 0, 1, 3, 4, 6, 7, 9 or 10
    /// (high 3 bits; up to 31,744), so short inputs can ask for strings that
    /// span several blocks.
    fn len(&mut self) -> usize {
        let b = self.u8().unwrap_or(0);
        usize::from(b & 31) << (usize::from(b >> 5) * 3 / 2)
    }

    /// `len()` bytes cycling through a pattern of up to 8 input bytes; with
    /// `no_nul`, NUL becomes `0x01`.
    fn string(&mut self, no_nul: bool) -> Vec<u8> {
        let n = self.len();
        let plen = usize::from(self.u8().unwrap_or(0) % 8) + 1;
        let mut pat = Vec::with_capacity(plen);
        for _ in 0..plen {
            let b = self.u8().unwrap_or(b'a');
            pat.push(if no_nul && b == 0 { 1 } else { b });
        }
        pat.iter().copied().cycle().take(n).collect()
    }
}

/// Decodes arbitrary bytes into a pool program (used by the fuzz target and
/// the random tests).
pub fn pool_ops_from_bytes(data: &[u8]) -> Vec<PoolOp> {
    let mut b = Bytes(data);
    let mut ops = Vec::new();
    while let Some(op) = b.u8() {
        ops.push(match op % 16 {
            0 => PoolOp::AppendChar(b.u8().unwrap_or(0)),
            1 => PoolOp::AppendChars(b.string(false)),
            2 => {
                let e = b.u8().unwrap_or(0);
                PoolOp::Append(e, b.string(false))
            }
            3 => {
                let e = b.u8().unwrap_or(0);
                PoolOp::StoreString(e, b.string(false))
            }
            4 => PoolOp::CopyString(b.string(true)),
            5 => PoolOp::CopyStringNoFinish(b.string(true)),
            6 => PoolOp::CopyStringN(b.string(false)),
            7 => PoolOp::AppendString(b.string(true)),
            8 => PoolOp::Grow,
            9 => PoolOp::GrowUntil(b.len() as u32),
            10 => PoolOp::Finish,
            11 => PoolOp::Discard,
            12 => PoolOp::Chop,
            13 => PoolOp::Clear,
            14 => PoolOp::FailNext(u32::from(b.u8().unwrap_or(0) % 8)),
            _ => PoolOp::LastChar,
        });
    }
    ops
}

/// Decodes arbitrary bytes into a hash table program. Besides single
/// operations, a bulk opcode inserts many generated keys, so short inputs
/// reach large tables (where the probe step is truncated to 8 bits).
pub fn hash_ops_from_bytes(data: &[u8]) -> Vec<HashOp> {
    const SIZES: [u32; 4] = [0, 24, 40, 56];
    let mut b = Bytes(data);
    let mut ops = Vec::new();
    while let Some(op) = b.u8() {
        match op % 16 {
            0..=6 => {
                // Short keys from a small alphabet collide and repeat.
                let n = usize::from(b.u8().unwrap_or(0) % 12);
                let key: Vec<u8> = (0..n)
                    .map(|_| b"abcxyz:_-.\xC3\xA9"[usize::from(b.u8().unwrap_or(0) % 12)])
                    .collect();
                let size = SIZES[usize::from(op >> 6)];
                ops.push(if op % 16 == 6 {
                    HashOp::Find(key)
                } else {
                    HashOp::Insert(key, size)
                });
            }
            7 => ops.push(HashOp::Find(b.string(true))),
            8 => ops.push(HashOp::Insert(b.string(true), 0)),
            9 => ops.push(HashOp::Iterate),
            10 => ops.push(HashOp::FailNext(u32::from(b.u8().unwrap_or(0) % 8))),
            11 => {
                if op >> 4 == 0xB {
                    ops.push(HashOp::Clear);
                } else {
                    ops.push(HashOp::Iterate);
                }
            }
            _ => {
                let count = usize::from(b.u8().unwrap_or(0)) * 4;
                let prefix = b.u8().unwrap_or(0) % 26 + b'a';
                for i in 0..count {
                    ops.push(HashOp::Insert(
                        format!("{}{i}", prefix as char).into_bytes(),
                        0,
                    ));
                }
            }
        }
    }
    ops
}
