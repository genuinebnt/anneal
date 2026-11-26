//! RECIPE 830 (bits): bit manipulation with REAL examples: flags, CIDR, packed headers, Snowflake/ULID ids, bit streams, chess bitboards, bitmask DP, XOR tricks, Rust pitfalls.
//! LEVEL: intermediate
//! Use in: permission systems, routers and firewalls, protocol parsers, id generators, compressors, game engines, DP interviews.   Run: cargo run --release --example bit_manipulation_real_world
//! COMMON USES: feature/permission flag sets, longest-prefix match in IP routing tables, network and file-format headers, time-ordered unique ids,
//! varint encodings (protobuf-style) and entropy-coder bit streams, chess/Go engines (bitboards), bitmap indexes and Bloom filters, bitmask DP, Sudoku/exact-cover solvers.
#![allow(dead_code)]
//!
//! WHAT: examples/algorithms/bit_tricks.rs is the *toolbox* (popcount, SWAR, Morton, PDEP/PEXT, clmul...). This file is the *applications*: sixteen
//! small, self-contained sections, each a real use of bits, each with an ORACLE (an obviously-correct slow version, an exhaustive loop, or seeded random
//! inputs). To avoid duplication it only re-derives a trick when the application needs it; see also
//! examples/algorithms/heuristics_zoo.rs (Held-Karp TSP as an exact oracle) and examples/data_structures/bitset_rank_select_roaring.rs (the full
//! rank/select + Roaring story; section 12 here is the 40-line version). Interview versions: examples/interview/intervals_greedy_bit_questions.rs.
//!
//! SECTIONS: 1 flag sets  2 CIDR + longest-prefix match  3 packed headers + toy CPU  4 Snowflake/ULID ids  5 varint/zigzag/gamma/Rice bit streams
//! 6 chess bitboards + perft  7 bitmask DP (TSP, subset sum, SOS)  8 Gray code, bit reversal, popcount (+ timing)  9 branchless tricks (exhaustive)
//! 10 XOR tricks  11 submasks and Gosper  12 bitmap index, Bloom, rank/select  13 Sudoku candidate masks  14 endianness  15 Rust pitfalls.
//!
//! RUST NOTES: shifting by >= the bit width PANICS in debug builds and is masked (wraps the count) in release: never write `1u32 << n` with n possibly 32;
//! use `checked_shl`, or branch (section 2 shows the prefix-mask case). `<<` on signed integers never checks lost bits, only the shift COUNT.
//! Edition 2024: `gen` is reserved, so generators are called `next_id`.
//! Read: https://graphics.stanford.edu/~seander/bithacks.html  https://www.chessprogramming.org/Bitboards  https://www.chessprogramming.org/Hyperbola_Quintessence
//! https://www.chessprogramming.org/Magic_Bitboards  https://www.chessprogramming.org/Perft_Results  https://github.com/twitter-archive/snowflake  https://github.com/ulid/spec
//! https://doc.rust-lang.org/std/primitive.u32.html  (Henry S. Warren, Hacker's Delight 2nd ed.; Knuth TAOCP 4A 7.1.3 cited by title only.)
//!
//! TRY NEXT:
//!   - [S] In `Cidr::mask`, replace the `low_bits` formulation with `u128::MAX << (width - len)` and run the CIDR section in a debug build: which prefix length panics, and why does release hide it?
//!   - [S] Predict, then measure: in section 8, how much faster is `pop_swar` than `pop_kernighan` on words from `sparse(5)` versus dense words? Change the generator and explain the crossover.
//!   - [M] Add castling to `Position::pseudo_moves` (track rights in `Position`, forbid passing through attacked squares) and extend section 6 to assert the published depth-5 start perft of 4,865,609.
//!   - [M] Port `BitWriter`/`BitReader` to write 64-bit words at a time (buffer in a u64 accumulator) and re-run the Rice round-trip property; measure the speedup versus the per-bit `put_bit`.
//!   - [A] PROJECT: build a tiny firewall: load a few thousand CIDR rules into `PrefixTrie`, aggregate them with `aggregate`, and compare lookup cost against a Roaring-style index from examples/data_structures/bitset_rank_select_roaring.rs; add a Bloom pre-filter and measure the false-positive cost.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::hint::black_box;
use std::ops::{BitAnd, BitOr, Not};
use std::time::Instant;

// =====================================================================================================================
// Plumbing: seeded xorshift.
// =====================================================================================================================
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    /// Random word with each bit set with probability about 2^-k (k=1: dense half, k=3: sparse).
    fn sparse(&mut self, k: u32) -> u64 {
        let mut w = self.next();
        for _ in 1..k {
            w &= self.next();
        }
        w
    }
}

/// Iterate the set bits of a word, lowest first: `x &= x - 1` clears the lowest set bit, `trailing_zeros` locates it.
/// Cost is O(popcount), not O(64): this is the loop behind every bitset iterator (Roaring, chess move generators, ...).
struct BitIter(u64);
impl Iterator for BitIter {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.0 == 0 {
            return None;
        }
        let i = self.0.trailing_zeros();
        self.0 &= self.0 - 1;
        Some(i)
    }
}

// =====================================================================================================================
// 1. Permission / feature flags: what the `bitflags` crate generates, written by hand (no macro) so you see the mechanism.
// =====================================================================================================================
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
struct Perms(u32);
impl Perms {
    const READ: Perms = Perms(1 << 0);
    const WRITE: Perms = Perms(1 << 1);
    const EXEC: Perms = Perms(1 << 2);
    const DELETE: Perms = Perms(1 << 3);
    const ADMIN: Perms = Perms(1 << 4);
    const ALL: Perms = Perms(0b1_1111);
    const NAMES: [&'static str; 5] = ["READ", "WRITE", "EXEC", "DELETE", "ADMIN"];
    /// Unknown bits are dropped: a flag type must never hold a combination it has no name for (bitflags `from_bits_truncate`).
    const fn from_bits_truncate(b: u32) -> Perms {
        Perms(b & Self::ALL.0)
    }
    /// `from_bits` refuses unknown bits instead (use when parsing untrusted input).
    const fn from_bits(b: u32) -> Option<Perms> {
        if b & !Self::ALL.0 == 0 {
            Some(Perms(b))
        } else {
            None
        }
    }
    const fn contains(self, o: Perms) -> bool {
        self.0 & o.0 == o.0
    }
    const fn intersects(self, o: Perms) -> bool {
        self.0 & o.0 != 0
    }
    const fn difference(self, o: Perms) -> Perms {
        Perms(self.0 & !o.0)
    }
    const fn toggle(self, o: Perms) -> Perms {
        Perms(self.0 ^ o.0)
    }
    fn names(self) -> Vec<&'static str> {
        BitIter(self.0 as u64)
            .map(|i| Self::NAMES[i as usize])
            .collect()
    }
}
impl BitOr for Perms {
    type Output = Perms;
    fn bitor(self, o: Perms) -> Perms {
        Perms(self.0 | o.0)
    }
}
impl BitAnd for Perms {
    type Output = Perms;
    fn bitand(self, o: Perms) -> Perms {
        Perms(self.0 & o.0)
    }
}
impl Not for Perms {
    type Output = Perms;
    fn not(self) -> Perms {
        Perms(!self.0 & Self::ALL.0)
    } // complement WITHIN the universe, not 32 bits
}

fn section1_flags() {
    // Oracle: a BTreeSet<u32> of flag indices. Exhaustive over all 32 x 32 pairs of flag sets.
    let to_set = |p: Perms| -> BTreeSet<u32> { (0..5).filter(|i| p.0 >> i & 1 == 1).collect() };
    for a in 0..32u32 {
        for b in 0..32u32 {
            let (pa, pb) = (Perms(a), Perms(b));
            let (sa, sb) = (to_set(pa), to_set(pb));
            assert_eq!(
                to_set(pa | pb),
                sa.union(&sb).copied().collect::<BTreeSet<_>>()
            );
            assert_eq!(
                to_set(pa & pb),
                sa.intersection(&sb).copied().collect::<BTreeSet<_>>()
            );
            assert_eq!(
                to_set(pa.difference(pb)),
                sa.difference(&sb).copied().collect::<BTreeSet<_>>()
            );
            assert_eq!(
                to_set(pa.toggle(pb)),
                sa.symmetric_difference(&sb)
                    .copied()
                    .collect::<BTreeSet<_>>()
            );
            assert_eq!(pa.contains(pb), sb.is_subset(&sa));
            assert_eq!(pa.intersects(pb), !sa.is_disjoint(&sb));
        }
        assert_eq!(
            to_set(!Perms(a)),
            (0..5).filter(|i| a >> i & 1 == 0).collect::<BTreeSet<_>>()
        );
    }
    assert_eq!(Perms::from_bits(0b100000), None);
    assert_eq!(Perms::from_bits_truncate(0xFFFF_FFFF), Perms::ALL);
    let rw = Perms::READ | Perms::WRITE;
    assert_eq!(rw.names(), ["READ", "WRITE"]);
    assert!(rw.contains(Perms::READ) && !rw.contains(Perms::READ | Perms::EXEC));
    // Iteration over set bits of arbitrary u64 words versus the 64-step loop.
    let mut r = Rng(0xA11CE);
    for k in 1..5 {
        for _ in 0..2000 {
            let w = r.sparse(k);
            let fast: Vec<u32> = BitIter(w).collect();
            let slow: Vec<u32> = (0..64).filter(|i| w >> i & 1 == 1).collect();
            assert_eq!(fast, slow);
            assert_eq!(fast.len() as u32, w.count_ones());
        }
    }
    println!(
        "1. flags: Perms {:?} = {:?}; BitIter ok on 8000 words",
        rw,
        rw.names()
    );
}

// =====================================================================================================================
// 2. IPv4 / IPv6 CIDR: masks, containment, aggregation, longest-prefix match (linear scan vs bitwise trie).
// One type for both: the address lives in the low `width` bits of a u128 (width 32 or 128).
// =====================================================================================================================
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
struct Cidr {
    addr: u128, // host bits already zeroed
    len: u8,
    width: u8,
}
/// Low `n` bits set. `1 << 128` would panic in debug, so n >= 128 is special-cased.
fn low_bits(n: u32) -> u128 {
    if n >= 128 {
        u128::MAX
    } else {
        (1u128 << n) - 1
    }
}
impl Cidr {
    fn new(addr: u128, len: u8, width: u8) -> Cidr {
        assert!(len <= width && width <= 128);
        let c = Cidr {
            addr: 0,
            len,
            width,
        };
        Cidr {
            addr: addr & c.mask(),
            ..c
        }
    }
    /// Network mask: `len` ones followed by host zeros, inside the width. len == 0 is the classic trap (`!0 << width` shift overflow).
    fn mask(&self) -> u128 {
        low_bits(self.width as u32) & !low_bits((self.width - self.len) as u32)
    }
    fn contains_addr(&self, a: u128) -> bool {
        a & self.mask() == self.addr
    }
    fn contains(&self, o: &Cidr) -> bool {
        o.len >= self.len && o.addr & self.mask() == self.addr
    }
    fn parse_v4(s: &str) -> Option<Cidr> {
        let (ip, len) = s.split_once('/')?;
        let ip: std::net::Ipv4Addr = ip.parse().ok()?;
        let len: u8 = len.parse().ok()?;
        (len <= 32).then(|| Cidr::new(u32::from(ip) as u128, len, 32))
    }
    fn show_v4(&self) -> String {
        format!(
            "{}/{}",
            std::net::Ipv4Addr::from(self.addr as u32),
            self.len
        )
    }
}

/// Remove covered prefixes, then merge sibling pairs into their parent, until nothing changes. Output is minimal for the covered address set.
fn aggregate(mut v: Vec<Cidr>) -> Vec<Cidr> {
    loop {
        v.sort();
        v.dedup();
        // Sorted by (addr, len): a covering prefix always precedes the prefixes it covers.
        let mut kept: Vec<Cidr> = Vec::new();
        for p in v {
            if kept.last().is_none_or(|l| !l.contains(&p)) {
                kept.push(p);
            }
        }
        // Siblings: same length, differ only in the last network bit, the first of them having that bit = 0.
        let mut merged: Vec<Cidr> = Vec::new();
        let mut changed = false;
        let mut i = 0;
        while i < kept.len() {
            if i + 1 < kept.len() {
                let (a, b) = (kept[i], kept[i + 1]);
                if a.len == b.len && a.len > 0 && a.addr ^ b.addr == 1u128 << (a.width - a.len) {
                    merged.push(Cidr {
                        len: a.len - 1,
                        ..a
                    });
                    changed = true;
                    i += 2;
                    continue;
                }
            }
            merged.push(kept[i]);
            i += 1;
        }
        v = merged;
        if !changed {
            return v;
        }
    }
}

/// Binary trie over address bits, MSB first. A node stores an optional value; lookup remembers the deepest value seen on the path.
struct PrefixTrie {
    child: Vec<[u32; 2]>, // 0 = none (root is index 0 and never a child)
    val: Vec<Option<u32>>,
    width: u8,
}
impl PrefixTrie {
    fn new(width: u8) -> Self {
        PrefixTrie {
            child: vec![[0, 0]],
            val: vec![None],
            width,
        }
    }
    fn bit(&self, addr: u128, i: u32) -> usize {
        (addr >> (self.width as u32 - 1 - i) & 1) as usize
    }
    fn insert(&mut self, p: Cidr, v: u32) {
        let mut n = 0usize;
        for i in 0..p.len as u32 {
            let b = self.bit(p.addr, i);
            if self.child[n][b] == 0 {
                self.child.push([0, 0]);
                self.val.push(None);
                self.child[n][b] = (self.child.len() - 1) as u32;
            }
            n = self.child[n][b] as usize;
        }
        self.val[n] = Some(v);
    }
    /// Longest-prefix match: (matched length, value). O(width) regardless of table size.
    fn lookup(&self, addr: u128) -> Option<(u32, u32)> {
        let (mut n, mut best) = (0usize, self.val[0].map(|v| (0, v)));
        for i in 0..self.width as u32 {
            let c = self.child[n][self.bit(addr, i)];
            if c == 0 {
                break;
            }
            n = c as usize;
            if let Some(v) = self.val[n] {
                best = Some((i + 1, v));
            }
        }
        best
    }
}

fn random_prefix(r: &mut Rng, width: u8) -> Cidr {
    let len = if width == 32 {
        r.range(4, 28)
    } else {
        r.range(8, 64)
    } as u8;
    let addr = ((r.next() as u128) << 64 | r.next() as u128) & low_bits(width as u32);
    // Cluster the high bits so prefixes overlap and siblings occur (otherwise aggregation has nothing to do).
    let clustered = addr & !(low_bits(width as u32) >> 3) | (addr & 0xFF) << (width as u32 - 11);
    Cidr::new(clustered, len, width)
}

fn section2_cidr() {
    assert_eq!(Cidr::new(0, 0, 32).mask(), 0);
    assert_eq!(Cidr::new(0, 32, 32).mask(), 0xFFFF_FFFF);
    assert_eq!(Cidr::new(0, 24, 32).mask(), 0xFFFF_FF00);
    assert_eq!(Cidr::new(0, 128, 128).mask(), u128::MAX);
    // Overflow demo: the std way to express "shift that may reach the width".
    assert_eq!(u32::MAX.checked_shl(32), None);
    let c = Cidr::parse_v4("10.1.2.77/24").unwrap();
    assert_eq!(c.show_v4(), "10.1.2.0/24");
    assert!(c.contains_addr(u32::from(std::net::Ipv4Addr::new(10, 1, 2, 255)) as u128));
    assert!(!c.contains_addr(u32::from(std::net::Ipv4Addr::new(10, 1, 3, 0)) as u128));
    assert!(Cidr::parse_v4("1.2.3.4/33").is_none());
    let v6: std::net::Ipv6Addr = "2001:db8::1".parse().unwrap();
    let p6 = Cidr::new(u128::from(v6), 32, 128);
    assert!(p6.contains_addr(u128::from(
        "2001:db8:ffff::9".parse::<std::net::Ipv6Addr>().unwrap()
    )));
    assert!(!p6.contains_addr(u128::from(
        "2001:db9::1".parse::<std::net::Ipv6Addr>().unwrap()
    )));
    // Hand-made aggregation: four /26 siblings collapse to one /24; a covered /32 disappears.
    let four: Vec<Cidr> = [
        "192.168.0.0/26",
        "192.168.0.64/26",
        "192.168.0.128/26",
        "192.168.0.192/26",
        "192.168.0.7/32",
    ]
    .iter()
    .map(|s| Cidr::parse_v4(s).unwrap())
    .collect();
    let agg = aggregate(four);
    assert_eq!(
        agg.iter().map(Cidr::show_v4).collect::<Vec<_>>(),
        ["192.168.0.0/24"]
    );

    let mut r = Rng(0xC1D8);
    for &width in &[32u8, 128] {
        for round in 0..30 {
            let n = 5 + (round * 7) % 120;
            let prefixes: Vec<Cidr> = (0..n).map(|_| random_prefix(&mut r, width)).collect();
            let agg = aggregate(prefixes.clone());
            assert!(agg.len() <= prefixes.len());
            // Aggregated prefixes are pairwise non-nested.
            for (i, a) in agg.iter().enumerate() {
                for b in &agg[i + 1..] {
                    assert!(!a.contains(b) && !b.contains(a));
                }
            }
            // Oracle 1: same set of covered addresses (probe near prefixes and uniformly).
            for _ in 0..600 {
                let base = prefixes[r.below(n as u64) as usize];
                let probe = if r.below(2) == 0 {
                    base.addr
                        | (((r.next() as u128) << 64 | r.next() as u128)
                            & !base.mask()
                            & low_bits(width as u32))
                } else {
                    ((r.next() as u128) << 64 | r.next() as u128) & low_bits(width as u32)
                };
                assert_eq!(
                    prefixes.iter().any(|p| p.contains_addr(probe)),
                    agg.iter().any(|p| p.contains_addr(probe))
                );
            }
            // Oracle 2: longest-prefix match, trie vs linear scan, on the UNIQUE normalised table.
            let table: HashMap<Cidr, u32> = prefixes
                .iter()
                .enumerate()
                .map(|(i, p)| (*p, i as u32))
                .collect();
            let mut trie = PrefixTrie::new(width);
            for (p, v) in &table {
                trie.insert(*p, *v);
            }
            for _ in 0..600 {
                let base = prefixes[r.below(n as u64) as usize];
                let probe = base.addr
                    | (((r.next() as u128) << 64 | r.next() as u128)
                        & !base.mask()
                        & low_bits(width as u32));
                let linear = table
                    .iter()
                    .filter(|(p, _)| p.contains_addr(probe))
                    .max_by_key(|(p, _)| p.len)
                    .map(|(p, v)| (p.len as u32, *v));
                assert_eq!(trie.lookup(probe), linear);
            }
        }
    }
    println!(
        "2. cidr: aggregation + trie LPM agree with linear scans (IPv4 and IPv6, 60 random tables)"
    );
}

// =====================================================================================================================
// 3. Packed headers: field get/put helper, TCP flags, IPv4 header words, checksum, and a 16-bit toy CPU encoding.
// =====================================================================================================================
/// Read `width` bits starting at bit `shift` (LSB = 0). Width < 32 only; a width-32 mask would need the same overflow care as CIDR.
fn get(word: u32, shift: u32, width: u32) -> u32 {
    (word >> shift) & ((1 << width) - 1)
}
/// Replace that field; every other bit is untouched. Property-tested below.
fn put(word: u32, shift: u32, width: u32, v: u32) -> u32 {
    let m = ((1u32 << width) - 1) << shift;
    (word & !m) | ((v << shift) & m)
}

mod tcp {
    pub const FIN: u8 = 1 << 0;
    pub const SYN: u8 = 1 << 1;
    pub const RST: u8 = 1 << 2;
    pub const PSH: u8 = 1 << 3;
    pub const ACK: u8 = 1 << 4;
    pub const URG: u8 = 1 << 5;
    pub const ECE: u8 = 1 << 6;
    pub const CWR: u8 = 1 << 7;
    pub fn describe(f: u8) -> String {
        const N: [&str; 8] = ["FIN", "SYN", "RST", "PSH", "ACK", "URG", "ECE", "CWR"];
        (0..8)
            .filter(|i| f >> i & 1 == 1)
            .map(|i| N[i])
            .collect::<Vec<_>>()
            .join("|")
    }
    /// Combinations a sane stack never sends: a firewall rule is literally a mask test.
    pub fn is_suspicious(f: u8) -> bool {
        (f & (SYN | FIN) == SYN | FIN)
            || (f & (SYN | RST) == SYN | RST)
            || f == 0
            || f == FIN | PSH | URG
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Ipv4Head {
    version: u8, // 4 bits
    ihl: u8,     // 4 bits
    dscp: u8,    // 6 bits
    ecn: u8,     // 2 bits
    total_len: u16,
    flags: u8,     // 3 bits (reserved, DF, MF)
    frag_off: u16, // 13 bits
}
impl Ipv4Head {
    fn pack(&self) -> [u32; 2] {
        let mut w0 = 0;
        w0 = put(w0, 28, 4, self.version as u32);
        w0 = put(w0, 24, 4, self.ihl as u32);
        w0 = put(w0, 18, 6, self.dscp as u32);
        w0 = put(w0, 16, 2, self.ecn as u32);
        w0 = put(w0, 0, 16, self.total_len as u32);
        // Word 1 is [identification:16][flags:3][frag_off:13]; identification is left zero here.
        let w1 = put(
            put(0, 13, 3, self.flags as u32),
            0,
            13,
            self.frag_off as u32,
        );
        [w0, w1]
    }
    fn unpack([w0, w1]: [u32; 2]) -> Self {
        Ipv4Head {
            version: get(w0, 28, 4) as u8,
            ihl: get(w0, 24, 4) as u8,
            dscp: get(w0, 18, 6) as u8,
            ecn: get(w0, 16, 2) as u8,
            total_len: get(w0, 0, 16) as u16,
            flags: get(w1, 13, 3) as u8,
            frag_off: get(w1, 0, 13) as u16,
        }
    }
}

/// RFC 1071 internet checksum: ones' complement of the ones' complement sum of 16-bit words (carry folded back in).
fn internet_checksum(bytes: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for ch in bytes.chunks(2) {
        let w = if ch.len() == 2 {
            u16::from_be_bytes([ch[0], ch[1]])
        } else {
            u16::from_be_bytes([ch[0], 0])
        };
        sum += w as u32;
        sum = (sum & 0xFFFF) + (sum >> 16); // end-around carry
    }
    !(sum as u16)
}

/// Toy 16-bit ISA. R: [op:4][rd:3][rs:3][rt:3][0:3]   I: [op:4][rd:3][rs:3][imm:6 signed]   J: [op:4][off:12 signed]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Instr {
    Add { rd: u8, rs: u8, rt: u8 },
    Sub { rd: u8, rs: u8, rt: u8 },
    Addi { rd: u8, rs: u8, imm: i8 },
    Load { rd: u8, rs: u8, imm: i8 },
    Jmp { off: i16 },
}
/// Sign-extend the low `bits` of `x`: shift the field to the top, then an ARITHMETIC right shift copies the sign bit down.
fn sext(x: u16, bits: u32) -> i16 {
    ((x << (16 - bits)) as i16) >> (16 - bits)
}
fn encode(i: Instr) -> u16 {
    let r = |op: u16, rd: u8, rs: u8, rt: u8| {
        op << 12 | (rd as u16) << 9 | (rs as u16) << 6 | (rt as u16) << 3
    };
    let im = |op: u16, rd: u8, rs: u8, imm: i8| {
        op << 12 | (rd as u16) << 9 | (rs as u16) << 6 | (imm as u16 & 0x3F)
    };
    match i {
        Instr::Add { rd, rs, rt } => r(0, rd, rs, rt),
        Instr::Sub { rd, rs, rt } => r(1, rd, rs, rt),
        Instr::Addi { rd, rs, imm } => im(2, rd, rs, imm),
        Instr::Load { rd, rs, imm } => im(3, rd, rs, imm),
        Instr::Jmp { off } => 4 << 12 | (off as u16 & 0xFFF),
    }
}
fn decode(w: u16) -> Option<Instr> {
    let (rd, rs, rt) = ((w >> 9 & 7) as u8, (w >> 6 & 7) as u8, (w >> 3 & 7) as u8);
    let imm = sext(w & 0x3F, 6) as i8;
    match w >> 12 {
        0 | 1 if w & 7 != 0 => None, // reserved padding bits must be zero (strict decoders reject, lenient ones ignore)
        0 => Some(Instr::Add { rd, rs, rt }),
        1 => Some(Instr::Sub { rd, rs, rt }),
        2 => Some(Instr::Addi { rd, rs, imm }),
        3 => Some(Instr::Load { rd, rs, imm }),
        4 => Some(Instr::Jmp {
            off: sext(w & 0xFFF, 12),
        }),
        _ => None,
    }
}

fn section3_headers() {
    assert_eq!(tcp::describe(tcp::SYN | tcp::ACK), "SYN|ACK");
    assert!(
        tcp::is_suspicious(tcp::SYN | tcp::FIN)
            && !tcp::is_suspicious(tcp::SYN)
            && !tcp::is_suspicious(tcp::ACK | tcp::PSH)
    );
    // put/get: round trip and "does not disturb neighbours", exhaustive on small widths, random on the rest.
    let mut r = Rng(0xFEED);
    for _ in 0..20_000 {
        let word = r.next() as u32;
        let width = 1 + r.below(16) as u32;
        let shift = r.below(33 - width as u64) as u32;
        let v = r.next() as u32 & ((1 << width) - 1);
        let w2 = put(word, shift, width, v);
        assert_eq!(get(w2, shift, width), v);
        let field = ((1u32 << width) - 1) << shift;
        assert_eq!(w2 & !field, word & !field);
    }
    for _ in 0..5000 {
        let h = Ipv4Head {
            version: r.below(16) as u8,
            ihl: r.below(16) as u8,
            dscp: r.below(64) as u8,
            ecn: r.below(4) as u8,
            total_len: r.next() as u16,
            flags: r.below(8) as u8,
            frag_off: r.below(8192) as u16,
        };
        assert_eq!(Ipv4Head::unpack(h.pack()), h);
    }
    // Wikipedia's worked IPv4 checksum example header (checksum field zeroed): expected 0xB861.
    let hdr = [
        0x45, 0x00, 0x00, 0x73, 0x00, 0x00, 0x40, 0x00, 0x40, 0x11, 0x00, 0x00, 0xC0, 0xA8, 0x00,
        0x01, 0xC0, 0xA8, 0x00, 0xC7,
    ];
    let ck = internet_checksum(&hdr);
    assert_eq!(ck, 0xB861);
    let mut full = hdr;
    full[10..12].copy_from_slice(&ck.to_be_bytes());
    assert_eq!(
        internet_checksum(&full),
        0,
        "a header carrying its own checksum sums to zero"
    );
    // Toy CPU: encode(decode(w)) == w for EVERY decodable 16-bit word (exhaustive), and decode(encode(i)) == i for every instruction.
    let mut valid = 0;
    for w in 0..=u16::MAX {
        if let Some(i) = decode(w) {
            valid += 1;
            assert_eq!(encode(i), w, "{w:#06x} -> {i:?}");
        }
    }
    for _ in 0..20_000 {
        let (rd, rs, rt) = (r.below(8) as u8, r.below(8) as u8, r.below(8) as u8);
        let imm = r.range(-32, 31) as i8;
        let i = match r.below(5) {
            0 => Instr::Add { rd, rs, rt },
            1 => Instr::Sub { rd, rs, rt },
            2 => Instr::Addi { rd, rs, imm },
            3 => Instr::Load { rd, rs, imm },
            _ => Instr::Jmp {
                off: r.range(-2048, 2047) as i16,
            },
        };
        assert_eq!(decode(encode(i)), Some(i));
    }
    assert_eq!(sext(0x3F, 6), -1);
    assert_eq!(sext(0x20, 6), -32);
    assert_eq!(sext(0x1F, 6), 31);
    println!(
        "3. headers: IPv4 pack/unpack + checksum {ck:#06x}; toy ISA: {valid} of 65536 words decode, all re-encode exactly"
    );
}

// =====================================================================================================================
// 4. Time-ordered 64-bit ids (Snowflake layout) and 128-bit ULIDs with Crockford base32.
// =====================================================================================================================
const WORKER_BITS: u32 = 10;
const SEQ_BITS: u32 = 12;
const TS_BITS: u32 = 41; // 41 + 10 + 12 = 63: the sign bit stays 0 so ids are positive i64 in Java/Postgres bigint.
const EPOCH_MS: u64 = 1_288_834_974_657; // Twitter's custom epoch, any fixed instant works

fn sf_pack(ts_ms: u64, worker: u64, seq: u64) -> u64 {
    debug_assert!(ts_ms < 1 << TS_BITS && worker < 1 << WORKER_BITS && seq < 1 << SEQ_BITS);
    ts_ms << (WORKER_BITS + SEQ_BITS) | worker << SEQ_BITS | seq
}
fn sf_unpack(id: u64) -> (u64, u64, u64) {
    (
        id >> (WORKER_BITS + SEQ_BITS),
        id >> SEQ_BITS & ((1 << WORKER_BITS) - 1),
        id & ((1 << SEQ_BITS) - 1),
    )
}
#[derive(Debug, PartialEq, Eq)]
enum IdError {
    ClockBackwards,
    SequenceExhausted, // 4096 ids already issued this millisecond: caller must wait for the next one
}
struct Snowflake {
    worker: u64,
    last_ms: u64,
    seq: u64,
}
impl Snowflake {
    fn new(worker: u64) -> Self {
        Snowflake {
            worker,
            last_ms: 0,
            seq: 0,
        }
    }
    /// `now_ms` is injected so tests are deterministic (never read the clock inside logic you want to test).
    fn next_id(&mut self, now_ms: u64) -> Result<u64, IdError> {
        if now_ms < self.last_ms {
            return Err(IdError::ClockBackwards);
        }
        let seq = if now_ms == self.last_ms {
            let s = self.seq + 1;
            if s >= 1 << SEQ_BITS {
                return Err(IdError::SequenceExhausted);
            }
            s
        } else {
            0
        };
        self.last_ms = now_ms;
        self.seq = seq;
        Ok(sf_pack(now_ms - EPOCH_MS, self.worker, seq))
    }
}

const B32: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ"; // Crockford: no I, L, O, U
fn ulid_encode(v: u128) -> String {
    // 26 chars * 5 bits = 130 bits for 128: the first char carries only 3.
    (0..26)
        .map(|i| B32[(v >> (125 - 5 * i) & 31) as usize] as char)
        .collect()
}
fn ulid_decode(s: &str) -> Option<u128> {
    if s.len() != 26 {
        return None;
    }
    let mut v: u128 = 0;
    for (i, c) in s.bytes().enumerate() {
        let d = B32.iter().position(|&x| x == c.to_ascii_uppercase())? as u128;
        if i == 0 && d > 7 {
            return None; // would overflow 128 bits
        }
        v = v << 5 | d;
    }
    Some(v)
}

fn section4_ids() {
    // Pack/unpack round trip on random fields, and numeric order == (ts, worker, seq) lexicographic order.
    let mut r = Rng(0x5107);
    let mut triples = Vec::new();
    for _ in 0..5000 {
        let t = (
            r.below(1 << 20),
            r.below(1 << WORKER_BITS),
            r.below(1 << SEQ_BITS),
        );
        let id = sf_pack(t.0 + EPOCH_MS, t.1, t.2); // adds the epoch back below in `next_id`; here we pack raw fields
        let raw = sf_pack(t.0, t.1, t.2);
        assert_eq!(sf_unpack(raw), t);
        triples.push((raw, t));
        let _ = id;
    }
    triples.sort_by_key(|x| x.0);
    assert!(
        triples.windows(2).all(|w| w[0].1 <= w[1].1),
        "id order must be (ts, worker, seq) order"
    );
    assert!(
        sf_pack((1 << TS_BITS) - 1, 1023, 4095) < 1 << 63,
        "max id still fits in i64"
    );
    // Generator: monotone per worker over a jittery clock, sequence exhaustion, clock regression.
    let mut g = Snowflake::new(7);
    let mut last = 0;
    let mut now = EPOCH_MS + 1000;
    let mut issued = 0;
    for _ in 0..50_000 {
        now += r.below(3); // 0, 1 or 2 ms steps: often several ids per ms
        let id = g.next_id(now).expect("at most 3 ids per ms here");
        assert!(id > last);
        last = id;
        issued += 1;
        assert_eq!(sf_unpack(id).1, 7);
    }
    let mut g = Snowflake::new(1);
    for _ in 0..4096 {
        g.next_id(EPOCH_MS + 5).unwrap();
    }
    assert_eq!(g.next_id(EPOCH_MS + 5), Err(IdError::SequenceExhausted));
    assert!(g.next_id(EPOCH_MS + 6).is_ok());
    assert_eq!(g.next_id(EPOCH_MS + 3), Err(IdError::ClockBackwards));
    // Different workers never collide even in the same millisecond.
    let mut seen = HashSet::new();
    let mut gens: Vec<Snowflake> = (0..16).map(Snowflake::new).collect();
    for ms in 0..200 {
        for gn in gens.iter_mut() {
            for _ in 0..r.below(5) {
                assert!(seen.insert(gn.next_id(EPOCH_MS + 10 + ms).unwrap()));
            }
        }
    }
    // ULID: round trip, lexicographic string order == numeric order, rejects bad input.
    let ulids: Vec<u128> = (0..3000)
        .map(|_| (r.next() as u128) << 64 | r.next() as u128)
        .collect();
    for &u in ulids.iter().chain([0, u128::MAX, 1].iter()) {
        let s = ulid_encode(u);
        assert_eq!(s.len(), 26);
        assert_eq!(ulid_decode(&s), Some(u));
        assert_eq!(ulid_decode(&s.to_lowercase()), Some(u));
    }
    for w in ulids.windows(2) {
        assert_eq!(w[0].cmp(&w[1]), ulid_encode(w[0]).cmp(&ulid_encode(w[1])));
    }
    assert_eq!(ulid_decode("80000000000000000000000000"), None);
    assert_eq!(ulid_decode("0000000000000000000000000I"), None);
    assert_eq!(ulid_encode(u128::MAX), "7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
    println!(
        "4. ids: {issued} monotone snowflakes, {} unique multi-worker ids, ULID order matches string order",
        seen.len()
    );
}

// =====================================================================================================================
// 5. Bit streams: LEB128 varint, zigzag, Elias-gamma and Golomb-Rice codes with a BitWriter / BitReader.
// =====================================================================================================================
fn varint_encode(mut v: u64, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push((v & 0x7F) as u8 | 0x80); // low 7 bits + continuation flag
        v >>= 7;
    }
    out.push(v as u8);
}
fn varint_decode(buf: &[u8]) -> Option<(u64, usize)> {
    let mut v = 0u64;
    for (i, &b) in buf.iter().enumerate() {
        if i >= 10 {
            return None; // u64 needs at most 10 groups of 7 bits
        }
        let part = (b & 0x7F) as u64;
        if i == 9 && part > 1 {
            return None; // would overflow 64 bits
        }
        v |= part << (7 * i);
        if b & 0x80 == 0 {
            return Some((v, i + 1));
        }
    }
    None // truncated
}
/// Zigzag maps signed to unsigned so small magnitudes stay small: 0,-1,1,-2,2 -> 0,1,2,3,4. `>> 63` on i64 is an arithmetic shift (all ones if negative).
fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}
fn unzigzag(z: u64) -> i64 {
    (z >> 1) as i64 ^ -((z & 1) as i64)
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    nbits: usize,
}
impl BitWriter {
    fn put_bit(&mut self, b: bool) {
        if self.nbits % 8 == 0 {
            self.bytes.push(0);
        }
        if b {
            *self.bytes.last_mut().unwrap() |= 0x80 >> (self.nbits % 8); // MSB-first within each byte
        }
        self.nbits += 1;
    }
    /// Write the low `n` bits of `v`, most significant first.
    fn put_bits(&mut self, v: u64, n: u32) {
        for i in (0..n).rev() {
            self.put_bit(v >> i & 1 == 1);
        }
    }
}
struct BitReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        BitReader { bytes, pos: 0 }
    }
    fn bit(&mut self) -> Option<bool> {
        let byte = *self.bytes.get(self.pos / 8)?;
        let b = byte & (0x80 >> (self.pos % 8)) != 0;
        self.pos += 1;
        Some(b)
    }
    fn bits(&mut self, n: u32) -> Option<u64> {
        let mut v = 0;
        for _ in 0..n {
            v = v << 1 | self.bit()? as u64;
        }
        Some(v)
    }
}
/// Elias gamma for n >= 1: floor(log2 n) zeros, then n in binary (which starts with the 1 that ends the zero run). Length 2*floor(log2 n)+1.
fn gamma_put(w: &mut BitWriter, n: u64) {
    assert!(n >= 1);
    let l = 63 - n.leading_zeros();
    w.put_bits(0, l);
    w.put_bits(n, l + 1);
}
fn gamma_get(r: &mut BitReader) -> Option<u64> {
    let mut zeros = 0;
    while !r.bit()? {
        zeros += 1;
        if zeros > 63 {
            return None;
        }
    }
    Some(1u64 << zeros | r.bits(zeros)?)
}
/// Golomb-Rice with divisor 2^k: quotient in unary (q ones, one zero), remainder in k bits. Optimal for geometric data (used in FLAC, lossless audio).
fn rice_put(w: &mut BitWriter, n: u64, k: u32) {
    for _ in 0..n >> k {
        w.put_bit(true);
    }
    w.put_bit(false);
    w.put_bits(n, k); // put_bits keeps only the low k bits: exactly the remainder
}
fn rice_get(r: &mut BitReader, k: u32) -> Option<u64> {
    let mut q = 0u64;
    while r.bit()? {
        q += 1;
    }
    Some(q << k | r.bits(k)?)
}

fn section5_streams() {
    let mut buf = Vec::new();
    varint_encode(300, &mut buf);
    assert_eq!(buf, [0xAC, 0x02], "the protobuf documentation example");
    let mut r = Rng(0xB175);
    // zigzag exhaustive over i16, edge cases over i64.
    for n in i16::MIN..=i16::MAX {
        assert_eq!(unzigzag(zigzag(n as i64)), n as i64);
    }
    for n in [i64::MIN, i64::MAX, 0, -1, 1] {
        assert_eq!(unzigzag(zigzag(n)), n);
    }
    assert_eq!([0, -1, 1, -2, 2].map(zigzag), [0, 1, 2, 3, 4]);
    // varint round trip over a stream of mixed magnitudes, plus malformed inputs.
    let values: Vec<u64> = (0..20_000).map(|_| r.next() >> r.below(64)).collect();
    let mut stream = Vec::new();
    for &v in &values {
        varint_encode(v, &mut stream);
    }
    let (mut pos, mut got) = (0, Vec::new());
    while pos < stream.len() {
        let (v, n) = varint_decode(&stream[pos..]).unwrap();
        got.push(v);
        pos += n;
    }
    assert_eq!(got, values);
    assert_eq!(varint_decode(&[0x80]), None, "truncated");
    assert_eq!(varint_decode(&[0xFF; 11]), None, "too long");
    let mut max = Vec::new();
    varint_encode(u64::MAX, &mut max);
    assert_eq!((max.len(), varint_decode(&max)), (10, Some((u64::MAX, 10))));
    // Gamma and Rice: round trip + gamma length formula + compression vs fixed 32-bit on geometric-ish data.
    let data: Vec<u64> = (0..20_000)
        .map(|_| (r.next().trailing_zeros() as u64) * 3 + r.below(4))
        .collect();
    let mut gw = BitWriter::default();
    for &x in &data {
        let before = gw.nbits;
        gamma_put(&mut gw, x + 1);
        assert_eq!(
            gw.nbits - before,
            2 * (63 - (x + 1).leading_zeros()) as usize + 1
        );
    }
    let mut gr = BitReader::new(&gw.bytes);
    for &x in &data {
        assert_eq!(gamma_get(&mut gr), Some(x + 1));
    }
    let mean = data.iter().sum::<u64>() / data.len() as u64;
    let k = (63 - mean.max(1).leading_zeros()).max(0);
    let mut rw = BitWriter::default();
    for &x in &data {
        rice_put(&mut rw, x, k);
    }
    let mut rr = BitReader::new(&rw.bytes);
    for &x in &data {
        assert_eq!(rice_get(&mut rr, k), Some(x));
    }
    let mut vb = Vec::new();
    data.iter().for_each(|&x| varint_encode(x, &mut vb));
    assert!(rw.bytes.len() < data.len() * 4 && gw.bytes.len() < data.len() * 4);
    println!(
        "5. streams: {} values: fixed u32 {} B, varint {} B, gamma {} B, Rice(k={k}) {} B",
        data.len(),
        data.len() * 4,
        vb.len(),
        gw.bytes.len(),
        rw.bytes.len()
    );
    assert_eq!(BitReader::new(&[]).bit(), None);
}

// =====================================================================================================================
// 6. Chess bitboards. Square index = rank*8 + file, a1 = 0, h8 = 63; one u64 per piece type.
//    Attack tables for knights/kings via shifts + file masks; sliders via Hyperbola Quintessence (no tables) and Magic bitboards (tables);
//    perft counts verify the whole move generator end to end against published values.
// =====================================================================================================================
const FILE_A: u64 = 0x0101_0101_0101_0101;
const FILE_B: u64 = FILE_A << 1;
const FILE_G: u64 = FILE_A << 6;
const FILE_H: u64 = FILE_A << 7;
const RANK_1: u64 = 0xFF;
const RANK_8: u64 = 0xFF << 56;

fn knight_attacks(b: u64) -> u64 {
    // Shift by 1 or 2 files first (masking the wrap-around), then by 2 or 1 ranks (<<16 / <<8). The file masks are the whole trick.
    let l1 = (b >> 1) & !FILE_H;
    let l2 = (b >> 2) & !(FILE_G | FILE_H);
    let r1 = (b << 1) & !FILE_A;
    let r2 = (b << 2) & !(FILE_A | FILE_B);
    let (h1, h2) = (l1 | r1, l2 | r2);
    (h1 << 16) | (h1 >> 16) | (h2 << 8) | (h2 >> 8)
}
fn king_attacks(b: u64) -> u64 {
    let sides = ((b << 1) & !FILE_A) | ((b >> 1) & !FILE_H);
    let row = b | sides;
    (row << 8) | (row >> 8) | sides
}
fn pawn_attacks(white: bool, b: u64) -> u64 {
    if white {
        ((b << 7) & !FILE_H) | ((b << 9) & !FILE_A)
    } else {
        ((b >> 7) & !FILE_A) | ((b >> 9) & !FILE_H)
    }
}
/// Oracle: coordinates and bounds checks, no bit tricks.
fn step_targets(sq: u32, deltas: &[(i32, i32)]) -> u64 {
    let (r, f) = ((sq / 8) as i32, (sq % 8) as i32);
    deltas
        .iter()
        .filter(|(dr, df)| (0..8).contains(&(r + dr)) && (0..8).contains(&(f + df)))
        .fold(0, |a, (dr, df)| a | 1u64 << ((r + dr) * 8 + f + df))
}
const KNIGHT_D: [(i32, i32); 8] = [
    (1, 2),
    (2, 1),
    (-1, 2),
    (-2, 1),
    (1, -2),
    (2, -1),
    (-1, -2),
    (-2, -1),
];
const KING_D: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
const ROOK_D: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const BISHOP_D: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// Oracle for sliding pieces: walk each ray until the first blocker (inclusive).
fn slow_slider(sq: u32, occ: u64, dirs: &[(i32, i32)]) -> u64 {
    let mut out = 0;
    for &(dr, df) in dirs {
        let (mut r, mut f) = ((sq / 8) as i32 + dr, (sq % 8) as i32 + df);
        while (0..8).contains(&r) && (0..8).contains(&f) {
            let bit = 1u64 << (r * 8 + f);
            out |= bit;
            if occ & bit != 0 {
                break;
            }
            r += dr;
            f += df;
        }
    }
    out
}

/// Hyperbola Quintessence: attacks along ONE line (mask excludes the slider) from the subtraction o - 2s, which ripples a borrow up to the first blocker
/// and flips everything between; the same on the bit-reversed board covers the other direction. XOR joins the two and the mask trims the rest.
fn line_attacks(occ: u64, sq: u32, line: u64) -> u64 {
    let s = 1u64 << sq;
    let o = occ & line;
    let fwd = o.wrapping_sub(s.wrapping_mul(2));
    let rev = o
        .reverse_bits()
        .wrapping_sub(s.reverse_bits().wrapping_mul(2));
    (fwd ^ rev.reverse_bits()) & line
}
struct Lines {
    rank: [u64; 64],
    file: [u64; 64],
    diag: [u64; 64],
    anti: [u64; 64],
}
fn line_mask(sq: u32, dr: i32, df: i32) -> u64 {
    slow_slider(sq, 0, &[(dr, df), (-dr, -df)])
}
impl Lines {
    fn new() -> Self {
        let mut l = Lines {
            rank: [0; 64],
            file: [0; 64],
            diag: [0; 64],
            anti: [0; 64],
        };
        for sq in 0..64 {
            l.rank[sq as usize] = line_mask(sq, 0, 1);
            l.file[sq as usize] = line_mask(sq, 1, 0);
            l.diag[sq as usize] = line_mask(sq, 1, 1);
            l.anti[sq as usize] = line_mask(sq, 1, -1);
        }
        l
    }
    fn rook(&self, sq: u32, occ: u64) -> u64 {
        line_attacks(occ, sq, self.file[sq as usize])
            | line_attacks(occ, sq, self.rank[sq as usize])
    }
    fn bishop(&self, sq: u32, occ: u64) -> u64 {
        line_attacks(occ, sq, self.diag[sq as usize])
            | line_attacks(occ, sq, self.anti[sq as usize])
    }
}

/// Magic bitboards: for each square keep the "relevant occupancy" mask (ray squares minus board edges, since an edge square never blocks anything further).
/// A multiplier ("magic") hashes every possible subset of that mask to a distinct-enough table index: index = ((occ & mask) * magic) >> (64 - bits).
/// Constructive collisions are allowed when the attack sets are equal. We search for magics with sparse random candidates, as everyone does.
struct Magic {
    mask: u64,
    magic: u64,
    shift: u32,
    table: Vec<u64>,
}
fn relevant_mask(sq: u32, dirs: &[(i32, i32)]) -> u64 {
    let (r0, f0) = ((sq / 8) as i32, (sq % 8) as i32);
    let mut m = 0;
    for &(dr, df) in dirs {
        let (mut r, mut f) = (r0 + dr, f0 + df);
        // Stop one square early: the last square on a ray is an edge in that direction.
        while (0..8).contains(&(r + dr)) && (0..8).contains(&(f + df)) {
            m |= 1u64 << (r * 8 + f);
            r += dr;
            f += df;
        }
    }
    m
}
fn find_magic(sq: u32, dirs: &[(i32, i32)], rng: &mut Rng) -> Magic {
    let mask = relevant_mask(sq, dirs);
    let bits = mask.count_ones();
    // Enumerate all subsets of the mask (carry-rippler: sub = (sub - mask) & mask visits every submask) with their true attacks.
    let mut subsets = Vec::new();
    let mut sub = 0u64;
    loop {
        subsets.push((sub, slow_slider(sq, sub, dirs)));
        sub = sub.wrapping_sub(mask) & mask;
        if sub == 0 {
            break;
        }
    }
    let mut table = vec![0u64; 1 << bits];
    let mut stamp = vec![0u32; 1 << bits];
    for attempt in 1u32.. {
        let magic = rng.sparse(3);
        if (mask.wrapping_mul(magic) >> 56).count_ones() < 6 {
            continue; // classic quick reject: the high byte should be well mixed
        }
        let ok = subsets.iter().all(|&(occ, att)| {
            let i = (occ.wrapping_mul(magic) >> (64 - bits)) as usize;
            if stamp[i] != attempt {
                stamp[i] = attempt;
                table[i] = att;
                true
            } else {
                table[i] == att
            }
        });
        if ok {
            return Magic {
                mask,
                magic,
                shift: 64 - bits,
                table,
            };
        }
    }
    unreachable!()
}
impl Magic {
    fn attacks(&self, occ: u64) -> u64 {
        self.table[((occ & self.mask).wrapping_mul(self.magic) >> self.shift) as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Move {
    from: u8,
    to: u8,
    piece: u8,
    promo: u8, // 0 = none, else piece index
    ep: bool,
}
const PAWN: usize = 0;
const KNIGHT: usize = 1;
const BISHOP: usize = 2;
const ROOK: usize = 3;
const QUEEN: usize = 4;
const KING: usize = 5;

struct Tables {
    knight: [u64; 64],
    king: [u64; 64],
    lines: Lines,
}
impl Tables {
    fn new() -> Self {
        let mut t = Tables {
            knight: [0; 64],
            king: [0; 64],
            lines: Lines::new(),
        };
        for sq in 0..64 {
            t.knight[sq] = knight_attacks(1 << sq);
            t.king[sq] = king_attacks(1 << sq);
        }
        t
    }
}
#[derive(Clone, Copy)]
struct Position {
    piece: [u64; 6],
    color: [u64; 2], // 0 = white, 1 = black
    side: usize,
    ep: Option<u8>,
}
impl Position {
    /// Minimal FEN: placement, side to move, (castling ignored: this generator has no castling), en passant square.
    fn from_fen(fen: &str) -> Position {
        let mut p = Position {
            piece: [0; 6],
            color: [0; 2],
            side: 0,
            ep: None,
        };
        let mut parts = fen.split_whitespace();
        for (i, row) in parts.next().unwrap().split('/').enumerate() {
            let rank = 7 - i;
            let mut file = 0;
            for c in row.chars() {
                if let Some(d) = c.to_digit(10) {
                    file += d as usize;
                    continue;
                }
                let kind = "pnbrqk".find(c.to_ascii_lowercase()).unwrap();
                let bit = 1u64 << (rank * 8 + file);
                p.piece[kind] |= bit;
                p.color[c.is_ascii_lowercase() as usize] |= bit;
                file += 1;
            }
        }
        p.side = (parts.next() == Some("b")) as usize;
        parts.next(); // castling field, ignored
        if let Some(e) = parts.next().filter(|e| *e != "-") {
            let b = e.as_bytes();
            p.ep = Some((b[1] - b'1') * 8 + (b[0] - b'a'));
        }
        p
    }
    fn occ(&self) -> u64 {
        self.color[0] | self.color[1]
    }
    fn attacked(&self, t: &Tables, sq: u32, by: usize) -> bool {
        let occ = self.occ();
        let theirs = |k: usize| self.piece[k] & self.color[by];
        // Pawn trick: squares from which a pawn of the OTHER colour would hit `sq` are exactly the squares an `by` pawn would attack it from.
        pawn_attacks(by == 1, 1u64 << sq) & theirs(PAWN) != 0
            || t.knight[sq as usize] & theirs(KNIGHT) != 0
            || t.king[sq as usize] & theirs(KING) != 0
            || t.lines.rook(sq, occ) & (theirs(ROOK) | theirs(QUEEN)) != 0
            || t.lines.bishop(sq, occ) & (theirs(BISHOP) | theirs(QUEEN)) != 0
    }
    fn in_check(&self, t: &Tables, side: usize) -> bool {
        let k = (self.piece[KING] & self.color[side]).trailing_zeros();
        self.attacked(t, k, side ^ 1)
    }
    fn piece_on(&self, sq: u32) -> Option<usize> {
        (0..6).find(|&k| self.piece[k] >> sq & 1 == 1)
    }
    fn pseudo_moves(&self, t: &Tables, out: &mut Vec<Move>) {
        let (me, them) = (self.side, self.side ^ 1);
        let (own, enemy, occ) = (self.color[me], self.color[them], self.occ());
        let push = |out: &mut Vec<Move>, from: u32, to: u32, piece: usize| {
            out.push(Move {
                from: from as u8,
                to: to as u8,
                piece: piece as u8,
                promo: 0,
                ep: false,
            })
        };
        for kind in [KNIGHT, BISHOP, ROOK, QUEEN, KING] {
            for from in BitIter(self.piece[kind] & own) {
                let targets = match kind {
                    KNIGHT => t.knight[from as usize],
                    KING => t.king[from as usize],
                    BISHOP => t.lines.bishop(from, occ),
                    ROOK => t.lines.rook(from, occ),
                    _ => t.lines.bishop(from, occ) | t.lines.rook(from, occ),
                } & !own;
                for to in BitIter(targets) {
                    push(out, from, to, kind);
                }
            }
        }
        let (fwd, start_rank, last_rank): (i32, u32, u64) = if me == 0 {
            (8, 1, RANK_8)
        } else {
            (-8, 6, RANK_1)
        };
        for from in BitIter(self.piece[PAWN] & own) {
            let one = (from as i32 + fwd) as u32;
            let mut dests: Vec<(u32, bool)> = Vec::new(); // (square, is_ep)
            if occ >> one & 1 == 0 {
                dests.push((one, false));
                let two = (from as i32 + 2 * fwd) as u32;
                if from / 8 == start_rank && occ >> two & 1 == 0 {
                    dests.push((two, false));
                }
            }
            for to in BitIter(pawn_attacks(me == 0, 1 << from) & enemy) {
                dests.push((to, false));
            }
            if let Some(e) = self.ep {
                if pawn_attacks(me == 0, 1 << from) >> e & 1 == 1 {
                    dests.push((e as u32, true));
                }
            }
            for (to, ep) in dests {
                if last_rank >> to & 1 == 1 {
                    for promo in [KNIGHT, BISHOP, ROOK, QUEEN] {
                        out.push(Move {
                            from: from as u8,
                            to: to as u8,
                            piece: PAWN as u8,
                            promo: promo as u8,
                            ep: false,
                        });
                    }
                } else {
                    out.push(Move {
                        from: from as u8,
                        to: to as u8,
                        piece: PAWN as u8,
                        promo: 0,
                        ep,
                    });
                }
            }
        }
    }
    fn make(&self, m: Move) -> Position {
        let mut p = *self;
        let (me, them) = (self.side, self.side ^ 1);
        let (from, to) = (1u64 << m.from, 1u64 << m.to);
        // Capture: remove whatever enemy piece stands on `to` (en passant removes the pawn BEHIND `to`).
        let cap_sq = if m.ep {
            if me == 0 { m.to - 8 } else { m.to + 8 }
        } else {
            m.to
        };
        let cap = 1u64 << cap_sq;
        for k in 0..6 {
            p.piece[k] &= !cap;
        }
        p.color[them] &= !cap;
        let kind = if m.promo != 0 {
            m.promo as usize
        } else {
            m.piece as usize
        };
        p.piece[m.piece as usize] &= !from;
        p.piece[kind] |= to;
        p.color[me] = (p.color[me] & !from) | to;
        p.ep = (m.piece as usize == PAWN && (m.to as i32 - m.from as i32).abs() == 16)
            .then(|| (m.from + m.to) / 2);
        p.side = them;
        p
    }
}
fn perft(t: &Tables, p: &Position, depth: u32) -> u64 {
    let mut moves = Vec::with_capacity(48);
    p.pseudo_moves(t, &mut moves);
    let mut n = 0;
    for m in moves {
        let q = p.make(m);
        if q.in_check(t, p.side) {
            continue; // pseudo-legal move left our own king attacked: illegal (handles pins, check evasion, king walking into attack)
        }
        n += if depth <= 1 {
            1
        } else {
            perft(t, &q, depth - 1)
        };
    }
    n
}

fn section6_chess() {
    let t = Tables::new();
    // Tables vs coordinate oracle on all squares, and full-board shift versions on multi-bit sets.
    for sq in 0..64u32 {
        assert_eq!(t.knight[sq as usize], step_targets(sq, &KNIGHT_D));
        assert_eq!(t.king[sq as usize], step_targets(sq, &KING_D));
        assert_eq!(
            pawn_attacks(true, 1 << sq),
            step_targets(sq, &[(1, 1), (1, -1)])
        );
        assert_eq!(
            pawn_attacks(false, 1 << sq),
            step_targets(sq, &[(-1, 1), (-1, -1)])
        );
    }
    let mut r = Rng(0xC4E55);
    for _ in 0..2000 {
        let b = r.sparse(3);
        let union_k = BitIter(b).fold(0, |a, s| a | t.knight[s as usize]);
        assert_eq!(
            knight_attacks(b),
            union_k,
            "set-wise attacks equal per-square union"
        );
    }
    assert_eq!(t.knight[0].count_ones(), 2);
    assert_eq!(t.king[27].count_ones(), 8);
    // Sliders: Hyperbola Quintessence vs ray walking on random occupancies; then magic bitboards vs the same oracle.
    let t0 = Instant::now();
    let rook_magics: Vec<Magic> = (0..64).map(|sq| find_magic(sq, &ROOK_D, &mut r)).collect();
    let bishop_magics: Vec<Magic> = (0..64)
        .map(|sq| find_magic(sq, &BISHOP_D, &mut r))
        .collect();
    let magic_time = t0.elapsed();
    let mut checked = 0;
    for _ in 0..4000 {
        let k = 1 + r.below(3) as u32;
        let occ = r.sparse(k);
        for sq in 0..64u32 {
            let (sr, sb) = (
                slow_slider(sq, occ, &ROOK_D),
                slow_slider(sq, occ, &BISHOP_D),
            );
            assert_eq!(
                t.lines.rook(sq, occ),
                sr,
                "hyperbola rook sq {sq} occ {occ:#x}"
            );
            assert_eq!(t.lines.bishop(sq, occ), sb, "hyperbola bishop sq {sq}");
            assert_eq!(rook_magics[sq as usize].attacks(occ), sr);
            assert_eq!(bishop_magics[sq as usize].attacks(occ), sb);
            checked += 1;
        }
    }
    let table_bytes: usize = rook_magics
        .iter()
        .chain(&bishop_magics)
        .map(|m| m.table.len() * 8)
        .sum();
    // Perft from the start position: published node counts (chessprogramming.org Perft Results). No castling/promotion/ep occurs before ply 5.
    let start = Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -");
    let expected = [20u64, 400, 8902, 197_281];
    let t1 = Instant::now();
    for (d, &e) in expected.iter().enumerate() {
        assert_eq!(
            perft(&t, &start, d as u32 + 1),
            e,
            "start position perft({})",
            d + 1
        );
    }
    let start_time = t1.elapsed();
    // "Position 3" of the CPW suite: no castling rights, but en passant, promotions and pins appear, so it exercises those code paths.
    let pos3 = Position::from_fen("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -");
    for (d, &e) in [14u64, 191, 2812, 43_238, 674_624].iter().enumerate() {
        assert_eq!(
            perft(&t, &pos3, d as u32 + 1),
            e,
            "position 3 perft({})",
            d + 1
        );
    }
    assert!(start.piece_on(4).is_some() && start.in_check(&t, 0) == false);
    println!(
        "6. chess: {checked} slider probes agree; magic tables {} KiB built in {:?}; start perft 1-4 = {:?} in {:?}; position-3 perft 1-5 ok",
        table_bytes / 1024,
        magic_time,
        expected,
        start_time
    );
}

// =====================================================================================================================
// 7. Bitmask DP: Held-Karp TSP, subset sum with a u128 as the reachable set, sum-over-subsets (SOS) zeta transform.
// =====================================================================================================================
fn tsp_held_karp(d: &[Vec<u32>]) -> u32 {
    let n = d.len();
    const INF: u32 = u32::MAX / 2;
    // dp[mask][j]: cheapest path starting at 0, visiting exactly `mask` (bit 0 always set), ending at j.
    let mut dp = vec![vec![INF; n]; 1 << n];
    dp[1][0] = 0;
    for mask in 1..1usize << n {
        if mask & 1 == 0 {
            continue;
        }
        for last in BitIter(mask as u64) {
            let cur = dp[mask][last as usize];
            if cur >= INF {
                continue;
            }
            for next in 0..n {
                if mask >> next & 1 == 0 {
                    let nm = mask | 1 << next;
                    dp[nm][next] = dp[nm][next].min(cur + d[last as usize][next]);
                }
            }
        }
    }
    (1..n).map(|j| dp[(1 << n) - 1][j] + d[j][0]).min().unwrap()
}
fn tsp_brute(d: &[Vec<u32>]) -> u32 {
    fn rec(d: &[Vec<u32>], cur: usize, left: &mut Vec<usize>, cost: u32, best: &mut u32) {
        if cost >= *best {
            return; // branch and bound: only prunes, never changes the answer
        }
        if left.is_empty() {
            *best = (*best).min(cost + d[cur][0]);
            return;
        }
        for i in 0..left.len() {
            let c = left.swap_remove(i);
            rec(d, c, left, cost + d[cur][c], best);
            left.push(c);
            let last = left.len() - 1;
            left.swap(i, last); // restore the original order
        }
    }
    let mut best = u32::MAX;
    rec(d, 0, &mut (1..d.len()).collect(), 0, &mut best);
    best
}
fn random_dist(r: &mut Rng, n: usize) -> Vec<Vec<u32>> {
    let mut d = vec![vec![0; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j {
                d[i][j] = r.range(1, 100) as u32;
            }
        }
    }
    d
}
/// Reachable subset sums as bits of a u128: sums up to 127. `reach |= reach << x` adds x to every reachable sum at once.
fn subset_sums_bits(xs: &[u32]) -> u128 {
    xs.iter().fold(1u128, |reach, &x| reach | reach << x)
}
fn subset_sums_dp(xs: &[u32], cap: usize) -> Vec<bool> {
    let mut ok = vec![false; cap + 1];
    ok[0] = true;
    for &x in xs {
        for s in (x as usize..=cap).rev() {
            ok[s] |= ok[s - x as usize];
        }
    }
    ok
}
/// f[mask] = sum of g[sub] over all sub subset-of mask, in O(n 2^n) instead of O(3^n).
fn sos_zeta(f: &mut [i64], n: usize) {
    for i in 0..n {
        for mask in 0..1usize << n {
            if mask >> i & 1 == 1 {
                f[mask] += f[mask ^ 1 << i];
            }
        }
    }
}
fn sos_mobius(f: &mut [i64], n: usize) {
    for i in 0..n {
        for mask in 0..1usize << n {
            if mask >> i & 1 == 1 {
                f[mask] -= f[mask ^ 1 << i];
            }
        }
    }
}

fn section7_dp() {
    let mut r = Rng(0x7511);
    for round in 0..60 {
        let n = 3 + round % 6; // 3..=8
        let d = random_dist(&mut r, n);
        assert_eq!(tsp_held_karp(&d), tsp_brute(&d), "n={n}");
    }
    let t0 = Instant::now();
    let d = random_dist(&mut r, 12);
    let (hk, bf) = (tsp_held_karp(&d), tsp_brute(&d));
    assert_eq!(hk, bf);
    let tsp_time = t0.elapsed();
    for _ in 0..500 {
        let xs: Vec<u32> = (0..r.below(12)).map(|_| r.range(1, 30) as u32).collect();
        let bits = subset_sums_bits(&xs);
        let dp = subset_sums_dp(&xs, 127);
        for s in 0..=127usize {
            // sums above 127 fall off the top of the u128 (shifted out), which is fine: we only ask for s <= 127
            assert_eq!(bits >> s & 1 == 1, dp[s], "sum {s} of {xs:?}");
        }
    }
    for n in [1usize, 4, 8, 10] {
        let g: Vec<i64> = (0..1usize << n).map(|_| r.range(-50, 50)).collect();
        let mut f = g.clone();
        sos_zeta(&mut f, n);
        for mask in 0..1usize << n {
            let mut s = 0;
            let mut sub = mask;
            loop {
                s += g[sub];
                if sub == 0 {
                    break;
                }
                sub = (sub - 1) & mask; // enumerate all submasks, largest first
            }
            assert_eq!(f[mask], s);
        }
        sos_mobius(&mut f, n);
        assert_eq!(f, g, "Mobius inverts zeta");
    }
    println!(
        "7. dp: TSP n=12 optimum {hk} (Held-Karp == branch&bound, {:?}); subset-sum bitset and SOS verified",
        tsp_time
    );
}

// =====================================================================================================================
// 8. Gray code, bit reversal (loop / table / swap ladder), popcount four ways, with a printed micro-benchmark.
// =====================================================================================================================
fn gray(x: u32) -> u32 {
    x ^ x >> 1
}
fn gray_inv(mut g: u32) -> u32 {
    let mut x = 0;
    while g != 0 {
        x ^= g; // x = g ^ g>>1 ^ g>>2 ...: prefix XOR from the top bit down
        g >>= 1;
    }
    x
}
fn rev_loop(mut x: u32) -> u32 {
    let mut out = 0;
    for _ in 0..32 {
        out = out << 1 | x & 1;
        x >>= 1;
    }
    out
}
/// Swap ladder: exchange adjacent bits, then pairs, nibbles, bytes, halves. The 0x5555.. masks select alternating groups.
fn rev_ladder(mut x: u32) -> u32 {
    x = (x >> 1 & 0x5555_5555) | (x & 0x5555_5555) << 1;
    x = (x >> 2 & 0x3333_3333) | (x & 0x3333_3333) << 2;
    x = (x >> 4 & 0x0F0F_0F0F) | (x & 0x0F0F_0F0F) << 4;
    x.swap_bytes() // the last two ladder rungs are exactly a byte swap
}
fn byte_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    for i in 0..256 {
        t[i] = (i as u8).reverse_bits();
    }
    t
}
fn rev_table(x: u32, t: &[u8; 256]) -> u32 {
    (t[(x & 0xFF) as usize] as u32) << 24
        | (t[(x >> 8 & 0xFF) as usize] as u32) << 16
        | (t[(x >> 16 & 0xFF) as usize] as u32) << 8
        | t[(x >> 24) as usize] as u32
}
fn pop_naive(mut x: u64) -> u32 {
    let mut c = 0;
    while x != 0 {
        c += (x & 1) as u32;
        x >>= 1;
    }
    c
}
fn pop_kernighan(mut x: u64) -> u32 {
    let mut c = 0;
    while x != 0 {
        x &= x - 1;
        c += 1;
    }
    c
}
fn pop_swar(mut x: u64) -> u32 {
    x -= x >> 1 & 0x5555_5555_5555_5555;
    x = (x & 0x3333_3333_3333_3333) + (x >> 2 & 0x3333_3333_3333_3333);
    x = (x + (x >> 4)) & 0x0F0F_0F0F_0F0F_0F0F;
    (x.wrapping_mul(0x0101_0101_0101_0101) >> 56) as u32
}

fn section8_gray_rev_pop() {
    // Gray code: exhaustive over u16: neighbours differ in one bit, cyclic, and invertible. Real use: rotary encoders, Karnaugh maps, hypercube walks.
    for x in 0..=u16::MAX as u32 {
        assert_eq!(gray_inv(gray(x)), x);
        assert_eq!((gray(x) ^ gray(x + 1)).count_ones(), 1);
    }
    assert_eq!(
        (gray(0xFFFF) ^ gray(0)).count_ones(),
        1,
        "the 16-bit Gray cycle closes"
    );
    assert_eq!(
        (0..8).map(gray).collect::<Vec<_>>(),
        [0, 1, 3, 2, 6, 7, 5, 4]
    );
    let table = byte_table();
    let mut r = Rng(0x6A4);
    let mut words: Vec<u64> = (0..1 << 16).map(|_| r.next()).collect();
    for x in 0..=u16::MAX as u32 {
        assert_eq!(rev_loop(x), x.reverse_bits());
        assert_eq!(rev_ladder(x), x.reverse_bits());
        assert_eq!(rev_table(x, &table), x.reverse_bits());
    }
    for &w in &words[..5000] {
        let x = w as u32;
        assert_eq!(
            (rev_loop(x), rev_ladder(x), rev_table(x, &table)),
            (x.reverse_bits(), x.reverse_bits(), x.reverse_bits())
        );
    }
    words.extend([0, 1, u64::MAX, 1 << 63]);
    for &w in &words {
        let c = w.count_ones();
        assert_eq!((pop_naive(w), pop_kernighan(w), pop_swar(w)), (c, c, c));
    }
    // Timing: printed only. black_box stops the optimiser from folding the loops.
    let words = black_box(words);
    let mut line = String::new();
    for (name, f) in [
        ("naive", pop_naive as fn(u64) -> u32),
        ("kernighan", pop_kernighan),
        ("swar", pop_swar),
        ("count_ones", |x: u64| x.count_ones()),
    ] {
        // Best of 5 repetitions (min filters scheduler noise). LLVM may recognise Kernighan/SWAR as `ctpop` and emit the same code as count_ones.
        let mut best = f64::MAX;
        for _ in 0..5 {
            let t0 = Instant::now();
            let mut s = 0u64;
            for &w in &words {
                s += f(black_box(w)) as u64;
            }
            black_box(s);
            best = best.min(t0.elapsed().as_nanos() as f64 / words.len() as f64);
        }
        line += &format!("{name} {best:.2} ns/op  ");
    }
    println!(
        "8. gray/reverse/popcount verified. popcount timing (machine-dependent, random words): {line}"
    );
}

// =====================================================================================================================
// 9. Branchless tricks, each verified exhaustively on 16-bit values (and 8-bit pairs for binary ops) and sampled over the full 64-bit range.
// =====================================================================================================================
fn abs_bl(x: i64) -> i64 {
    let m = x >> 63; // 0 or -1
    (x ^ m).wrapping_sub(m)
}
fn min_bl(x: i64, y: i64) -> i64 {
    y ^ ((x ^ y) & -((x < y) as i64))
}
fn max_bl(x: i64, y: i64) -> i64 {
    x ^ ((x ^ y) & -((x < y) as i64))
}
fn sign_bl(x: i64) -> i64 {
    (x >> 63) | (x.wrapping_neg() as u64 >> 63) as i64
}
fn lowest_bit(x: u64) -> u64 {
    x & x.wrapping_neg()
}
/// Smallest power of two >= x (x in 0..=2^63; 0 maps to 1 like `next_power_of_two`). Smear the top set bit downwards, then add one.
fn round_up_pow2(x: u64) -> u64 {
    let mut v = x.max(1) - 1;
    for s in [1, 2, 4, 8, 16, 32] {
        v |= v >> s;
    }
    v + 1
}
/// Average without the overflow of (a + b) / 2: shared bits plus half the differing bits.
fn avg_floor(a: u64, b: u64) -> u64 {
    (a & b) + ((a ^ b) >> 1)
}
fn is_pow2(x: u64) -> bool {
    x != 0 && x & (x - 1) == 0
}
fn cond_negate(x: i64, flag: bool) -> i64 {
    let m = -(flag as i64);
    (x ^ m).wrapping_sub(m)
}

fn section9_branchless() {
    for v in i16::MIN..=i16::MAX {
        let x = v as i64;
        assert_eq!(abs_bl(x), x.abs());
        assert_eq!(sign_bl(x), x.signum());
        assert_eq!(cond_negate(x, true), -x);
        assert_eq!(cond_negate(x, false), x);
        let u = v as u16 as u64;
        assert_eq!(
            lowest_bit(u),
            if u == 0 { 0 } else { 1 << u.trailing_zeros() }
        );
        assert_eq!(round_up_pow2(u), u.next_power_of_two());
        assert_eq!(is_pow2(u), u.count_ones() == 1);
    }
    for a in i8::MIN..=i8::MAX {
        for b in i8::MIN..=i8::MAX {
            let (x, y) = (a as i64, b as i64);
            assert_eq!((min_bl(x, y), max_bl(x, y)), (x.min(y), x.max(y)));
        }
    }
    for a in 0..=u8::MAX as u64 {
        for b in 0..=u8::MAX as u64 {
            assert_eq!(avg_floor(a, b), (a + b) / 2);
        }
    }
    let mut r = Rng(0xB4A7C);
    for _ in 0..200_000 {
        let (x, y) = (
            r.next() as i64 >> r.below(64),
            r.next() as i64 >> r.below(64),
        );
        assert_eq!(abs_bl(x), x.wrapping_abs());
        assert_eq!(sign_bl(x), x.signum());
        assert_eq!((min_bl(x, y), max_bl(x, y)), (x.min(y), x.max(y)));
        let (u, v) = (r.next() >> r.below(64), r.next() >> r.below(64));
        assert_eq!(avg_floor(u, v), ((u as u128 + v as u128) / 2) as u64);
        assert_eq!(
            lowest_bit(u),
            if u == 0 { 0 } else { 1 << u.trailing_zeros() }
        );
        let p = u >> 1; // keep within 2^63 so next_power_of_two does not overflow
        assert_eq!(round_up_pow2(p), p.next_power_of_two());
    }
    // The classic edge: abs(i64::MIN) has no positive counterpart; the trick wraps exactly like wrapping_abs.
    assert_eq!(abs_bl(i64::MIN), i64::MIN);
    assert_eq!(sign_bl(i64::MIN), -1);
    println!(
        "9. branchless: abs/min/max/sign/lowest/pow2/average exhaustive on 16-bit (8-bit pairs) + 200k random 64-bit"
    );
}

// =====================================================================================================================
// 10. XOR tricks: swap, single number, missing number, parity, and the XOR doubly linked list.
// =====================================================================================================================
fn xor_swap(v: &mut [u32], i: usize, j: usize) {
    v[i] ^= v[j];
    v[j] ^= v[i];
    v[i] ^= v[j];
}
fn single_number(xs: &[u32]) -> u32 {
    xs.iter().fold(0, |a, &x| a ^ x)
} // pairs cancel (x ^ x = 0), identity is 0
fn missing_number(xs: &[u32]) -> u32 {
    let n = xs.len() as u32;
    (0..=n).fold(0, |a, i| a ^ i) ^ single_number(xs)
}
fn parity(mut x: u64) -> u32 {
    x ^= x >> 32;
    x ^= x >> 16;
    x ^= x >> 8;
    x ^= x >> 4;
    x ^= x >> 2;
    x ^= x >> 1;
    (x & 1) as u32
}
/// XOR linked list: each node stores prev XOR next (as arena indices; 0 is the null sentinel). Walking needs the previous node to recover the next one.
/// In C this halves pointer storage; in Rust it needs raw pointers, so the educational version uses indices into a Vec (and the real one is `unsafe`).
struct XorList {
    nodes: Vec<(u32, u32)>, // (value, link) ; node 0 is the sentinel
    head: u32,
    tail: u32,
}
impl XorList {
    fn new() -> Self {
        XorList {
            nodes: vec![(0, 0)],
            head: 0,
            tail: 0,
        }
    }
    fn push_back(&mut self, v: u32) {
        let id = self.nodes.len() as u32;
        self.nodes.push((v, self.tail)); // prev = tail, next = 0
        if self.tail != 0 {
            self.nodes[self.tail as usize].1 ^= id; // tail's next was 0, so xor-ing in `id` sets next = id
        } else {
            self.head = id;
        }
        self.tail = id;
    }
    fn walk(&self, from_head: bool) -> Vec<u32> {
        let (mut cur, mut prev) = (if from_head { self.head } else { self.tail }, 0u32);
        let mut out = Vec::new();
        while cur != 0 {
            out.push(self.nodes[cur as usize].0);
            let next = self.nodes[cur as usize].1 ^ prev;
            prev = cur;
            cur = next;
        }
        out
    }
}

fn section10_xor() {
    let mut r = Rng(0x0DD);
    let mut v = vec![5, 9, 5];
    xor_swap(&mut v, 0, 1);
    assert_eq!(v, [9, 5, 5]);
    xor_swap(&mut v, 1, 1);
    assert_eq!(
        v[1], 0,
        "PITFALL: xor-swapping an element with itself zeroes it (std::mem::swap / slice::swap have no such bug)"
    );
    for _ in 0..2000 {
        let n = 1 + r.below(30) as usize;
        let pairs: Vec<u32> = (0..n).map(|_| r.below(1000) as u32).collect();
        let lone = r.below(1000) as u32 + 1000;
        let mut xs: Vec<u32> = pairs.iter().chain(&pairs).copied().chain([lone]).collect();
        for i in (1..xs.len()).rev() {
            xs.swap(i, r.below(i as u64 + 1) as usize);
        }
        assert_eq!(single_number(&xs), lone);
        // missing number among 0..=n
        let m = n as u32;
        let hole = r.below(m as u64 + 1) as u32;
        let mut full: Vec<u32> = (0..=m).filter(|&x| x != hole).collect();
        for i in (1..full.len()).rev() {
            full.swap(i, r.below(i as u64 + 1) as usize);
        }
        assert_eq!(missing_number(&full), hole);
        let w = r.next();
        assert_eq!(parity(w), w.count_ones() & 1);
    }
    let mut l = XorList::new();
    let vals: Vec<u32> = (0..50).map(|_| r.below(1000) as u32).collect();
    vals.iter().for_each(|&x| l.push_back(x));
    assert_eq!(l.walk(true), vals);
    assert_eq!(
        l.walk(false),
        vals.iter().rev().copied().collect::<Vec<_>>()
    );
    println!(
        "10. xor: swap pitfall demonstrated, single/missing number and parity verified, XOR list walks both ways"
    );
}

// =====================================================================================================================
// 11. Submask enumeration and Gosper's hack (next k-subset in lexicographic-by-value order).
// =====================================================================================================================
fn gosper(x: u64) -> u64 {
    // Next larger integer with the same popcount: ripple the lowest block of ones up by one place, put the leftover ones at the bottom.
    let c = x & x.wrapping_neg();
    let r = x + c;
    (((r ^ x) >> 2) / c) | r
}
fn binom(n: u64, k: u64) -> u64 {
    (0..k).fold(1, |a, i| a * (n - i) / (i + 1))
}

fn section11_subsets() {
    // All submasks of m: 2^popcount(m) of them, strictly decreasing, each a real subset.
    for m in 0u32..1 << 10 {
        let mut subs = Vec::new();
        let mut s = m;
        loop {
            subs.push(s);
            if s == 0 {
                break;
            }
            s = (s - 1) & m;
        }
        assert_eq!(subs.len(), 1 << m.count_ones());
        assert!(subs.windows(2).all(|w| w[0] > w[1]) && subs.iter().all(|s| s & !m == 0));
    }
    // Sum over all masks of 2^popcount(mask) == 3^n: the cost of "for each mask, for each submask".
    assert_eq!(
        (0u32..1 << 10).map(|m| 1u64 << m.count_ones()).sum::<u64>(),
        3u64.pow(10)
    );
    // Gosper: enumerates exactly the k-subsets of n, in increasing numeric order, count = C(n, k).
    for n in 1..=14u64 {
        for k in 1..=n {
            let limit = 1u64 << n;
            let (mut x, mut got) = ((1u64 << k) - 1, Vec::new());
            while x < limit {
                got.push(x);
                x = gosper(x);
            }
            let want: Vec<u64> = (0..limit).filter(|v| v.count_ones() as u64 == k).collect();
            assert_eq!(got, want);
            assert_eq!(got.len() as u64, binom(n, k));
        }
    }
    println!(
        "11. subsets: submask loop and Gosper's hack agree with filters for all n <= 14 (C(14,7) = {})",
        binom(14, 7)
    );
}

// =====================================================================================================================
// 12. Bitmap index (word-parallel AND/OR/ANDNOT), Bloom filter, and a sampled rank/select bitvector.
// =====================================================================================================================
#[derive(Clone, PartialEq, Debug)]
struct Bitmap {
    w: Vec<u64>,
    n: usize,
}
impl Bitmap {
    fn new(n: usize) -> Self {
        Bitmap {
            w: vec![0; n.div_ceil(64)],
            n,
        }
    }
    fn set(&mut self, i: usize) {
        self.w[i / 64] |= 1 << (i % 64);
    }
    fn get(&self, i: usize) -> bool {
        self.w[i / 64] >> (i % 64) & 1 == 1
    }
    fn and(&self, o: &Bitmap) -> Bitmap {
        Bitmap {
            w: self.w.iter().zip(&o.w).map(|(a, b)| a & b).collect(),
            n: self.n,
        }
    }
    fn or(&self, o: &Bitmap) -> Bitmap {
        Bitmap {
            w: self.w.iter().zip(&o.w).map(|(a, b)| a | b).collect(),
            n: self.n,
        }
    }
    fn and_not(&self, o: &Bitmap) -> Bitmap {
        Bitmap {
            w: self.w.iter().zip(&o.w).map(|(a, b)| a & !b).collect(),
            n: self.n,
        }
    }
    fn count(&self) -> usize {
        self.w.iter().map(|x| x.count_ones() as usize).sum()
    }
    fn rows(&self) -> Vec<usize> {
        self.w
            .iter()
            .enumerate()
            .flat_map(|(i, &x)| BitIter(x).map(move |b| i * 64 + b as usize))
            .collect()
    }
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ x >> 30).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ x >> 27).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ x >> 31
}
struct Bloom {
    bits: Vec<u64>,
    m: u64,
    k: u32,
}
impl Bloom {
    fn new(m: u64, k: u32) -> Self {
        Bloom {
            bits: vec![0; m.div_ceil(64) as usize],
            m,
            k,
        }
    }
    /// Kirsch-Mitzenmacher double hashing: k probe positions from two hashes, h1 + i*h2, with no loss in asymptotic false-positive rate.
    fn probes(&self, key: u64) -> impl Iterator<Item = u64> + use<> {
        let (h1, h2, m, k) = (
            splitmix(key),
            splitmix(key ^ 0xDEAD_BEEF) | 1,
            self.m,
            self.k,
        );
        (0..k as u64).map(move |i| h1.wrapping_add(i.wrapping_mul(h2)) % m)
    }
    fn insert(&mut self, key: u64) {
        for p in self.probes(key) {
            self.bits[(p / 64) as usize] |= 1 << (p % 64);
        }
    }
    fn maybe_contains(&self, key: u64) -> bool {
        self.probes(key)
            .all(|p| self.bits[(p / 64) as usize] >> (p % 64) & 1 == 1)
    }
}

/// Rank/select with a cumulative count sampled every 4 words: rank scans at most 3 words, select binary-searches the samples then scans.
struct RankSelect {
    words: Vec<u64>,
    samples: Vec<u32>, // samples[b] = ones in words[..4*b]
}
impl RankSelect {
    fn new(words: Vec<u64>) -> Self {
        let mut samples = Vec::new();
        let mut total = 0;
        for (i, w) in words.iter().enumerate() {
            if i % 4 == 0 {
                samples.push(total);
            }
            total += w.count_ones();
        }
        RankSelect { words, samples }
    }
    /// Number of ones strictly below position i.
    fn rank(&self, i: usize) -> u32 {
        let (wi, b) = (i / 64, i % 64);
        let start = wi / 4 * 4;
        let mut c = self.samples[wi / 4]
            + self.words[start..wi]
                .iter()
                .map(|w| w.count_ones())
                .sum::<u32>();
        if b > 0 {
            c += (self.words[wi] & ((1u64 << b) - 1)).count_ones(); // b in 1..=63, so the shift is safe
        }
        c
    }
    /// Position of the k-th one (0-based).
    fn select(&self, k: u32) -> Option<usize> {
        let blk = self.samples.partition_point(|&s| s <= k).checked_sub(1)?;
        let mut left = k - self.samples[blk];
        for wi in blk * 4..(blk * 4 + 4).min(self.words.len()) {
            let c = self.words[wi].count_ones();
            if left < c {
                let mut w = self.words[wi];
                for _ in 0..left {
                    w &= w - 1; // drop the lowest `left` ones; the next one is the answer
                }
                return Some(wi * 64 + w.trailing_zeros() as usize);
            }
            left -= c;
        }
        None
    }
}

fn section12_bitmaps() {
    let mut r = Rng(0xB17A);
    // Bitmap index over 5000 rows: country in 0..4, plan in 0..3, active flag. Query = AND/OR/ANDNOT of column bitmaps.
    let n = 5000;
    let rows: Vec<(u8, u8, bool)> = (0..n)
        .map(|_| (r.below(4) as u8, r.below(3) as u8, r.below(5) < 2))
        .collect();
    let mut by_country = vec![Bitmap::new(n); 4];
    let mut by_plan = vec![Bitmap::new(n); 3];
    let mut active = Bitmap::new(n);
    for (i, &(c, p, a)) in rows.iter().enumerate() {
        by_country[c as usize].set(i);
        by_plan[p as usize].set(i);
        if a {
            active.set(i);
        }
    }
    // country == 2 AND (plan == 1 OR plan == 2) AND NOT active
    let q = by_country[2]
        .and(&by_plan[1].or(&by_plan[2]))
        .and_not(&active);
    let scan: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|&(_, &(c, p, a))| c == 2 && (p == 1 || p == 2) && !a)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(q.rows(), scan);
    assert_eq!(q.count(), scan.len());
    assert!(scan.iter().all(|&i| q.get(i)));
    // Bloom filter: never a false negative; false-positive rate near (1 - e^{-kn/m})^k.
    let (m, k, items) = (80_000u64, 6u32, 8_000u64);
    let mut bf = Bloom::new(m, k);
    (0..items).for_each(|i| bf.insert(i * 7919));
    assert!((0..items).all(|i| bf.maybe_contains(i * 7919)));
    let trials = 200_000u64;
    let fp = (0..trials)
        .filter(|&i| bf.maybe_contains(1 << 40 | i))
        .count() as f64
        / trials as f64;
    let theory = (1.0 - (-(k as f64) * items as f64 / m as f64).exp()).powi(k as i32);
    assert!(fp < theory * 2.0 + 0.001, "fp {fp} vs theory {theory}");
    // Rank/select vs naive scans on sparse and dense words, awkward lengths.
    for k_sparse in [1u32, 3] {
        for nwords in [1usize, 3, 4, 5, 17, 64] {
            let words: Vec<u64> = (0..nwords).map(|_| r.sparse(k_sparse)).collect();
            let rs = RankSelect::new(words.clone());
            let ones: Vec<usize> = words
                .iter()
                .enumerate()
                .flat_map(|(i, &w)| BitIter(w).map(move |b| i * 64 + b as usize))
                .collect();
            for i in 0..nwords * 64 {
                assert_eq!(rs.rank(i) as usize, ones.iter().filter(|&&p| p < i).count());
            }
            for (kth, &pos) in ones.iter().enumerate() {
                assert_eq!(rs.select(kth as u32), Some(pos));
            }
            assert_eq!(rs.select(ones.len() as u32), None);
        }
    }
    println!(
        "12. bitmaps: index query matched {} rows; Bloom fp {fp:.5} (theory {theory:.5}); rank/select verified",
        scan.len()
    );
}

// =====================================================================================================================
// 13. Sudoku with candidate bitmasks: row/col/box masks of 9 bits, pick the cell with fewest candidates (MRV), backtrack.
// =====================================================================================================================
struct Sudoku {
    grid: [u8; 81],
    row: [u16; 9],
    col: [u16; 9],
    boxm: [u16; 9],
}
impl Sudoku {
    fn parse(s: &str) -> Sudoku {
        let mut sd = Sudoku {
            grid: [0; 81],
            row: [0; 9],
            col: [0; 9],
            boxm: [0; 9],
        };
        for (i, c) in s.bytes().enumerate() {
            if c != b'0' && c != b'.' {
                assert!(sd.place(i, c - b'0'), "contradictory puzzle");
            }
        }
        sd
    }
    fn unit(i: usize) -> (usize, usize, usize) {
        (i / 9, i % 9, i / 27 * 3 + i % 9 / 3)
    }
    fn candidates(&self, i: usize) -> u16 {
        let (r, c, b) = Self::unit(i);
        !(self.row[r] | self.col[c] | self.boxm[b]) & 0x1FF // bit d-1 set = digit d is still possible
    }
    fn place(&mut self, i: usize, d: u8) -> bool {
        if self.candidates(i) >> (d - 1) & 1 == 0 {
            return false;
        }
        let (r, c, b) = Self::unit(i);
        let bit = 1u16 << (d - 1);
        self.grid[i] = d;
        self.row[r] |= bit;
        self.col[c] |= bit;
        self.boxm[b] |= bit;
        true
    }
    fn unplace(&mut self, i: usize) {
        let (r, c, b) = Self::unit(i);
        let bit = 1u16 << (self.grid[i] - 1);
        self.row[r] ^= bit;
        self.col[c] ^= bit;
        self.boxm[b] ^= bit;
        self.grid[i] = 0;
    }
    /// Counts solutions up to `limit` (limit 2 proves uniqueness) and records the first one.
    fn solve(&mut self, limit: usize, first: &mut Option<[u8; 81]>) -> usize {
        // MRV: the empty cell with the fewest candidates; popcount is the heuristic, zero candidates means dead end.
        let best = (0..81)
            .filter(|&i| self.grid[i] == 0)
            .min_by_key(|&i| self.candidates(i).count_ones());
        let Some(i) = best else {
            first.get_or_insert(self.grid);
            return 1;
        };
        let mut found = 0;
        for bit in BitIter(self.candidates(i) as u64) {
            self.place(i, bit as u8 + 1);
            found += self.solve(limit - found, first);
            self.unplace(i);
            if found >= limit {
                break;
            }
        }
        found
    }
    fn is_valid_solution(g: &[u8; 81]) -> bool {
        let full = 0x1FFu16;
        (0..9).all(|k| {
            let row = (0..9).fold(0u16, |a, c| a | 1 << (g[k * 9 + c].wrapping_sub(1)));
            let col = (0..9).fold(0u16, |a, r| a | 1 << (g[r * 9 + k].wrapping_sub(1)));
            let bx = (0..9).fold(0u16, |a, j| {
                a | 1 << (g[(k / 3 * 3 + j / 3) * 9 + k % 3 * 3 + j % 3].wrapping_sub(1))
            });
            row == full && col == full && bx == full
        })
    }
}

fn section13_sudoku() {
    let puzzle =
        "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
    let solution =
        "534678912672195348198342567859761423426853791713924856961537284287419635345286179";
    let mut sd = Sudoku::parse(puzzle);
    let mut first = None;
    let t0 = Instant::now();
    assert_eq!(sd.solve(2, &mut first), 1, "puzzle has a unique solution");
    let g = first.unwrap();
    assert_eq!(
        g.iter().map(|d| (b'0' + d) as char).collect::<String>(),
        solution
    );
    assert!(Sudoku::is_valid_solution(&g));
    // Oracle by construction: blank random cells of a valid solution; every solver result must be a valid grid consistent with the givens.
    let mut r = Rng(0x5D0C);
    let base: Vec<u8> = solution.bytes().map(|c| c - b'0').collect();
    let mut solved = 0;
    for _ in 0..40 {
        let mut p = base.clone();
        for _ in 0..45 {
            p[r.below(81) as usize] = 0;
        }
        let text: String = p.iter().map(|d| (b'0' + d) as char).collect();
        let mut s = Sudoku::parse(&text);
        let mut f = None;
        assert!(s.solve(1, &mut f) >= 1);
        let g = f.unwrap();
        assert!(Sudoku::is_valid_solution(&g));
        assert!((0..81).all(|i| p[i] == 0 || p[i] == g[i]));
        solved += 1;
    }
    // An unsolvable puzzle: two 5s in one row cannot be placed.
    let mut bad = Sudoku::parse(
        "500000000000000000000000000000000000000000000000000000000000000000000000000000000",
    );
    assert!(!bad.place(1, 5));
    println!(
        "13. sudoku: classic puzzle solved uniquely in {:?}; {solved} random blanked grids re-solved validly",
        t0.elapsed()
    );
}

// =====================================================================================================================
// 14. Endianness and byte order.
// =====================================================================================================================
fn section14_endianness() {
    let x: u32 = 0x1234_5678;
    assert_eq!(x.to_be_bytes(), [0x12, 0x34, 0x56, 0x78]);
    assert_eq!(x.to_le_bytes(), [0x78, 0x56, 0x34, 0x12]);
    assert_eq!(u32::from_be_bytes([0x12, 0x34, 0x56, 0x78]), x);
    assert_eq!(x.swap_bytes(), 0x7856_3412);
    // Network byte order is big-endian: to_be() is a no-op on big-endian hosts and a swap on little-endian ones.
    assert_eq!(
        x.to_be(),
        if cfg!(target_endian = "little") {
            x.swap_bytes()
        } else {
            x
        }
    );
    assert_eq!(
        x.to_ne_bytes(),
        if cfg!(target_endian = "little") {
            x.to_le_bytes()
        } else {
            x.to_be_bytes()
        }
    );
    let mut r = Rng(0xE4D1);
    for _ in 0..10_000 {
        let v = r.next();
        assert_eq!(u64::from_be_bytes(v.to_be_bytes()), v);
        assert_eq!(u64::from_le_bytes(v.to_be_bytes()), v.swap_bytes());
        assert_eq!(v.swap_bytes().swap_bytes(), v);
        // Manual shift-and-or assembly equals from_be_bytes: what you write when you parse a header byte by byte (and what LLVM turns into one load + bswap).
        let b = v.to_be_bytes();
        let manual = b.iter().fold(0u64, |a, &byte| a << 8 | byte as u64);
        assert_eq!(manual, v);
    }
    // Reading an unaligned integer out of a byte slice without unsafe: slice -> array via try_into.
    let buf = [0u8, 1, 2, 3, 4, 5, 6, 7, 8];
    let v = u32::from_le_bytes(buf[3..7].try_into().unwrap());
    assert_eq!(v, 0x0605_0403);
    println!(
        "14. endianness: to/from_{{be,le}}_bytes, swap_bytes, to_be verified on this {}-endian host",
        if cfg!(target_endian = "little") {
            "little"
        } else {
            "big"
        }
    );
}

// =====================================================================================================================
// 15. Rust pitfalls with shifts, signed numbers and the checked_/wrapping_/overflowing_/saturating_ families.
// =====================================================================================================================
fn section15_pitfalls() {
    let n = black_box(32u32);
    // `1u32 << n` with n = 32 panics in debug ("attempt to shift left with overflow") but silently becomes `1 << 0` in release. Both are bugs: be explicit.
    assert_eq!(1u32.checked_shl(n), None);
    assert_eq!(
        1u32.wrapping_shl(n),
        1,
        "wrapping_shl reduces the COUNT modulo the width, it does not shift the bits out"
    );
    assert_eq!(1u32.overflowing_shl(n), (1, true));
    assert_eq!(1u32.checked_shl(31), Some(1 << 31));
    // Bits shifted out are silently lost; only the count is checked.
    assert_eq!(0xFFu8 << black_box(4), 0xF0);
    assert_eq!(
        1i32 << black_box(31),
        i32::MIN,
        "<< on signed ints can flip the sign bit with no panic"
    );
    // Shifts: `>>` is arithmetic for signed, logical for unsigned.
    assert_eq!(-8i32 >> 1, -4);
    assert_eq!((-8i32 as u32) >> 1, 0x7FFF_FFFC);
    assert_eq!(-1i32 >> 31, -1);
    // Rotate keeps every bit; shift drops them.
    assert_eq!(0x8000_0001u32.rotate_left(1), 0x0000_0003);
    assert_eq!(0x8000_0001u32 << 1, 0x0000_0002);
    assert_eq!(0x1u32.rotate_right(1), 0x8000_0000);
    assert_eq!(
        0x1234_5678u32.rotate_left(32),
        0x1234_5678,
        "rotate counts are taken modulo the width: no panic"
    );
    // Arithmetic families on the edge.
    assert_eq!(i32::MAX.checked_add(1), None);
    assert_eq!(i32::MAX.wrapping_add(1), i32::MIN);
    assert_eq!(i32::MAX.overflowing_add(1), (i32::MIN, true));
    assert_eq!(i32::MAX.saturating_add(1), i32::MAX);
    assert_eq!(i32::MIN.checked_abs(), None);
    assert_eq!(i32::MIN.unsigned_abs(), 1 << 31);
    assert_eq!(i32::MIN.checked_neg(), None);
    assert_eq!(0u32.checked_sub(1), None);
    assert_eq!(0u32.wrapping_sub(1), u32::MAX);
    assert_eq!(5u8.saturating_sub(9), 0);
    // `as` casts truncate or reinterpret silently; `try_from` reports.
    assert_eq!(300i32 as u8, 44);
    assert_eq!(-1i32 as u32, u32::MAX);
    assert_eq!(
        -1i8 as u8 as u32, 255,
        "sign-extends first when going signed -> wider: -1i8 as u32 would be u32::MAX"
    );
    assert_eq!(-1i8 as u32, u32::MAX);
    assert!(u8::try_from(300i32).is_err());
    // Comparators: `a - b` as an Ordering overflows for i32 extremes; use cmp.
    let (a, b) = (black_box(i32::MIN), black_box(1i32));
    assert!(
        a.wrapping_sub(b) > 0,
        "i32::MIN - 1 wraps to MAX, so a - b > 0 would say MIN > 1"
    );
    assert!(a.cmp(&b).is_lt());
    // Precedence: `==` binds tighter than `&` in C but NOT in Rust? In Rust, `&` binds tighter than `==`; still parenthesise for readers.
    let (flags, bit) = (0b101u8, 0b100u8);
    assert!(flags & bit == bit);
    assert_eq!(1 + 2 << 3, 24, "+ binds tighter than <<: (1 + 2) << 3");
    // Debug-vs-release is a real difference: `cfg!(debug_assertions)` tells which one this run is.
    println!(
        "15. pitfalls: shift-count, signed shifts, rotate, checked/wrapping families, casts verified (debug_assertions = {})",
        cfg!(debug_assertions)
    );
}

fn main() {
    section1_flags();
    section2_cidr();
    section3_headers();
    section4_ids();
    section5_streams();
    section6_chess();
    section7_dp();
    section8_gray_rev_pop();
    section9_branchless();
    section10_xor();
    section11_subsets();
    section12_bitmaps();
    section13_sudoku();
    section14_endianness();
    section15_pitfalls();
    println!("all bit-manipulation sections verified");
}
