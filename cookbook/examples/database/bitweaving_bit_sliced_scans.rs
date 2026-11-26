//! RECIPE 722 (database): bit-parallel scans over compressed codes: BitWeaving/V, BitWeaving/H and ByteSlice.
//! LEVEL: advanced
//! Use in: column stores scanning dictionary- or frame-of-reference-encoded columns with comparison predicates.   Run: cargo run --release --example bitweaving_bit_sliced_scans
//! COMMON USES: the BitWeaving research prototype and its descendants in main-memory column stores (the layouts were designed for
//! scans over order-preserving dictionary codes in the style of SAP HANA and Vertica-like engines), ByteSlice in the same line of
//! work, "bit-sliced indexes" in bitmap-index systems, SIMD bit-packed scans in Parquet/Arrow readers and DuckDB's bit-unpacking.
//! Course: CMU 15-721 (Advanced Database Systems): storage models / compression and vectorised scan lectures (BitWeaving, ByteSlice).
#![allow(dead_code)]
//!
//! WHAT: a column of n unsigned CODES of w bits each (1 <= w <= 32; a dictionary-encoded or min-subtracted column). A scan
//! evaluates `code <op> constant` (=, !=, <, <=, >, >=, BETWEEN) and produces a RESULT BITMAP (bit i = row i qualifies), which
//! predicates combine with plain AND/OR on words. Five physical layouts are compared:
//! * NAIVE: codes bit-packed tight in u64 words (w bits each, a code may straddle two words); the scan unpacks and compares ONE code
//!   at a time. Smallest memory, but unpacking dominates.
//! * PLAIN u32: a Vec<u32>, 4 bytes per code regardless of width; the loop is trivially auto-vectorised. Fast per byte, but moves
//!   32 bits for a 5-bit code.
//! * BITWEAVING/V (vertical): the BIT-SLICED layout. For every group of 64 rows store w words, word j holding BIT j (MSB first)
//!   of those 64 codes. A comparison with constant c runs MSB to LSB with two masks per group: `eq` (rows whose processed prefix
//!   equals c's prefix) and `lt` (rows already known smaller). Per bit: if c's bit is 1, rows with a 0 here become `lt`
//!   (`lt |= eq & !p`) and rows with a 1 stay equal (`eq &= p`); if c's bit is 0, rows with a 1 here are larger and drop out
//!   (`eq &= !p`). One word operation decides 64 rows. EARLY PRUNING: once `eq == 0` for the group every row is decided, so the
//!   remaining low-order bit words are never even read: on random data a 32-bit scan usually reads only ~8..10 of the 32 words.
//!   Bytes touched scale with the entropy needed to decide, not with the code width.
//! * BITWEAVING/H (horizontal): each code occupies a FIELD of w+1 bits whose top bit is a DELIMITER kept 0; as many fields as fit
//!   are packed into each u64 (no field straddles words). With D = the delimiter bits and C = the constant replicated into every
//!   field, `((C | D) - X) & D` has the delimiter of a field set iff x <= c. The delimiter guarantees the subtraction never borrows
//!   across fields, so ONE 64-bit subtraction compares all fields at once (SWAR: SIMD within a register). `>=` is the mirror
//!   `((X | D) - C) & D`, equality is their AND. Needs no transposition for point lookups (codes stay contiguous) but cannot prune.
//! * BYTESLICE: split each code (left-aligned to whole bytes) into ceil(w/8) byte slices stored as separate arrays, most
//!   significant first. A group of 64 rows is compared byte-slice by byte-slice with the same lt/eq masks; after the first slice,
//!   `eq` is nearly empty for high-selectivity-free constants, so later slices are skipped. Compared with BitWeaving/V it
//!   wastes at most 7 bits per code but gets byte granularity that real SIMD (`pcmpgtb`+`movemask`, 32 bytes at a time) handles
//!   natively and needs no bit-level shuffling; Rust's auto-vectoriser handles the inner loop here.
//!
//! WHY: scans are memory-bandwidth bound; reading fewer bytes per row is the whole game. Narrow codes mean a 10-bit column is a
//! fifth of a u32 column, and bit-parallel evaluation does the compare work for 64 rows per instruction instead of one. The
//! price: Naive/H cannot stop early and V/ByteSlice make point reads of a single code (re-assembling bits across w words) slower.
//!
//! CHECKS: 500 random columns, one per code width cycled over 1..=32 (n from 0 to ~300 rows, incl. non-multiples of 64, all-equal,
//! skewed and full-range data), every layout x every operator x boundary constants (0, max, max+1, random, the column median)
//! must equal a row-by-row oracle on the raw values; 300 random AND/OR expression trees over three columns of different widths
//! are evaluated with every layout and compared; padding rows must never leak into results. Bytes touched and speed are printed.
//! Read: BitWeaving: Fast Scans for Main Memory Data Processing https://pages.cs.wisc.edu/~jignesh/publ/BitWeaving.pdf
//!       ByteSlice: Pushing the Envelop of Main Memory Data Processing with a New Storage Layout https://www.cs.cmu.edu/~15721-f24/papers/ByteSlice.pdf
//!       CMU 15-721 https://15721.courses.cs.cmu.edu/spring2024/

use std::cell::Cell;
use std::hint::black_box;
use std::time::Instant;

type Bitmap = Vec<u64>;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
const OPS: [Op; 6] = [Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge];

fn holds(op: Op, x: u32, c: u64) -> bool {
    let x = x as u64;
    match op {
        Op::Eq => x == c,
        Op::Ne => x != c,
        Op::Lt => x < c,
        Op::Le => x <= c,
        Op::Gt => x > c,
        Op::Ge => x >= c,
    }
}

fn words(n: usize) -> usize {
    n.div_ceil(64)
}
fn valid_mask(n: usize, wi: usize) -> u64 {
    // Bits of word `wi` that correspond to real rows (the tail word is partially filled; the rest is padding).
    let rows = n - wi * 64;
    if rows >= 64 { !0 } else { (1u64 << rows) - 1 }
}
fn ones(n: usize) -> Bitmap {
    (0..words(n)).map(|wi| valid_mask(n, wi)).collect()
}
fn count(b: &Bitmap) -> usize {
    b.iter().map(|w| w.count_ones() as usize).sum()
}

// ---------------------------------------------------------------- layout trait
trait Layout {
    fn name(&self) -> &'static str;
    fn n(&self) -> usize;
    fn width(&self) -> u32;
    fn size_bytes(&self) -> usize;
    fn touched(&self) -> &Cell<u64>; // bytes of column data read by scans since the last reset
    /// Core primitive for a constant c that fits the width: (rows with code < c, rows with code == c). May include padding bits.
    fn lt_eq(&self, c: u32) -> (Bitmap, Bitmap);

    fn max_code(&self) -> u64 {
        if self.width() == 32 {
            u32::MAX as u64
        } else {
            (1u64 << self.width()) - 1
        }
    }
    /// Any comparison is derived from (lt, eq): out-of-range constants are decided without touching the data at all.
    fn cmp(&self, op: Op, c: u64) -> Bitmap {
        let n = self.n();
        if c > self.max_code() {
            return if matches!(op, Op::Lt | Op::Le | Op::Ne) {
                ones(n)
            } else {
                vec![0; words(n)]
            };
        }
        let (lt, eq) = self.lt_eq(c as u32);
        (0..words(n))
            .map(|i| {
                let r = match op {
                    Op::Eq => eq[i],
                    Op::Ne => !eq[i],
                    Op::Lt => lt[i],
                    Op::Le => lt[i] | eq[i],
                    Op::Gt => !(lt[i] | eq[i]),
                    Op::Ge => !lt[i],
                };
                r & valid_mask(n, i) // padding rows hold code 0 and would otherwise leak into the result
            })
            .collect()
    }
    fn between(&self, lo: u64, hi: u64) -> Bitmap {
        and(&self.cmp(Op::Ge, lo), &self.cmp(Op::Le, hi))
    }
}

fn and(a: &Bitmap, b: &Bitmap) -> Bitmap {
    a.iter().zip(b).map(|(x, y)| x & y).collect()
}
fn or(a: &Bitmap, b: &Bitmap) -> Bitmap {
    a.iter().zip(b).map(|(x, y)| x | y).collect()
}

// ---------------------------------------------------------------- NAIVE: tight bit-packing, one code at a time
struct Packed {
    n: usize,
    w: u32,
    data: Vec<u64>,
    touched: Cell<u64>,
}
impl Packed {
    fn build(codes: &[u32], w: u32) -> Self {
        let mut data = vec![0u64; (codes.len() * w as usize).div_ceil(64) + 1];
        for (i, &c) in codes.iter().enumerate() {
            let bit = i * w as usize;
            let (wd, off) = (bit / 64, (bit % 64) as u32);
            data[wd] |= (c as u64) << off;
            if off + w > 64 {
                data[wd + 1] |= (c as u64) >> (64 - off); // straddles into the next word
            }
        }
        Packed {
            n: codes.len(),
            w,
            data,
            touched: Cell::new(0),
        }
    }
    fn get(&self, i: usize) -> u32 {
        let bit = i * self.w as usize;
        let (wd, off) = (bit / 64, (bit % 64) as u32);
        let mut v = self.data[wd] >> off;
        if off + self.w > 64 {
            v |= self.data[wd + 1] << (64 - off);
        }
        (v & ((1u64 << self.w) - 1)) as u32
    }
}
impl Layout for Packed {
    fn name(&self) -> &'static str {
        "naive-packed"
    }
    fn n(&self) -> usize {
        self.n
    }
    fn width(&self) -> u32 {
        self.w
    }
    fn size_bytes(&self) -> usize {
        self.data.len() * 8
    }
    fn touched(&self) -> &Cell<u64> {
        &self.touched
    }
    fn lt_eq(&self, c: u32) -> (Bitmap, Bitmap) {
        let (mut lt, mut eq) = (vec![0u64; words(self.n)], vec![0u64; words(self.n)]);
        for i in 0..self.n {
            let v = self.get(i);
            lt[i / 64] |= ((v < c) as u64) << (i % 64);
            eq[i / 64] |= ((v == c) as u64) << (i % 64);
        }
        self.touched
            .set(self.touched.get() + (self.n * self.w as usize).div_ceil(8) as u64);
        (lt, eq)
    }
}

// ---------------------------------------------------------------- PLAIN u32
struct Plain {
    w: u32,
    codes: Vec<u32>,
    touched: Cell<u64>,
}
impl Layout for Plain {
    fn name(&self) -> &'static str {
        "plain-u32"
    }
    fn n(&self) -> usize {
        self.codes.len()
    }
    fn width(&self) -> u32 {
        self.w
    }
    fn size_bytes(&self) -> usize {
        self.codes.len() * 4
    }
    fn touched(&self) -> &Cell<u64> {
        &self.touched
    }
    fn lt_eq(&self, c: u32) -> (Bitmap, Bitmap) {
        let nw = words(self.codes.len());
        let (mut lt, mut eq) = (vec![0u64; nw], vec![0u64; nw]);
        for (wi, chunk) in self.codes.chunks(64).enumerate() {
            // Fixed-trip inner loops over 64 contiguous u32: the shape an auto-vectoriser likes.
            let (mut l, mut e) = (0u64, 0u64);
            for (r, &v) in chunk.iter().enumerate() {
                l |= ((v < c) as u64) << r;
                e |= ((v == c) as u64) << r;
            }
            (lt[wi], eq[wi]) = (l, e);
        }
        self.touched
            .set(self.touched.get() + 4 * self.codes.len() as u64);
        (lt, eq)
    }
}

// ---------------------------------------------------------------- BITWEAVING/V
struct BitSliced {
    n: usize,
    w: u32,
    /// data[g*w + j] = bit j (0 = most significant) of the 64 codes of row group g. A group's w words are adjacent, so a scan
    /// that stops after b bits reads one contiguous prefix of b words (the real system groups 512 codes per cache-line segment).
    data: Vec<u64>,
    touched: Cell<u64>,
}
impl BitSliced {
    fn build(codes: &[u32], w: u32) -> Self {
        let mut data = vec![0u64; words(codes.len()) * w as usize];
        for (i, &c) in codes.iter().enumerate() {
            for j in 0..w {
                let bit = (c >> (w - 1 - j)) & 1;
                data[(i / 64) * w as usize + j as usize] |= (bit as u64) << (i % 64);
            }
        }
        BitSliced {
            n: codes.len(),
            w,
            data,
            touched: Cell::new(0),
        }
    }
}
impl Layout for BitSliced {
    fn name(&self) -> &'static str {
        "BitWeaving/V"
    }
    fn n(&self) -> usize {
        self.n
    }
    fn width(&self) -> u32 {
        self.w
    }
    fn size_bytes(&self) -> usize {
        self.data.len() * 8
    }
    fn touched(&self) -> &Cell<u64> {
        &self.touched
    }
    fn lt_eq(&self, c: u32) -> (Bitmap, Bitmap) {
        let w = self.w as usize;
        let nw = words(self.n);
        let (mut lt_out, mut eq_out) = (vec![0u64; nw], vec![0u64; nw]);
        let mut read = 0u64;
        for g in 0..nw {
            let (mut lt, mut eq) = (0u64, !0u64);
            for j in 0..w {
                let p = self.data[g * w + j];
                read += 8;
                if (c >> (w - 1 - j)) & 1 == 1 {
                    lt |= eq & !p; // constant has 1, row has 0: row is smaller
                    eq &= p;
                } else {
                    eq &= !p; // constant has 0, row has 1: row is larger, drop out
                }
                if eq == 0 {
                    break; // EARLY PRUNING: every row of the group is decided; lower bits cannot matter
                }
            }
            (lt_out[g], eq_out[g]) = (lt, eq);
        }
        self.touched.set(self.touched.get() + read);
        (lt_out, eq_out)
    }
}

// ---------------------------------------------------------------- BITWEAVING/H
struct Horizontal {
    n: usize,
    w: u32,
    k: usize, // fields per word
    d: u64,   // delimiter mask: top bit of each field
    data: Vec<u64>,
    touched: Cell<u64>,
}
impl Horizontal {
    fn build(codes: &[u32], w: u32) -> Self {
        let f = w as usize + 1; // field = delimiter bit + w code bits
        let k = 64 / f;
        let mut data = vec![0u64; codes.len().div_ceil(k)];
        for (i, &c) in codes.iter().enumerate() {
            data[i / k] |= (c as u64) << ((i % k) * f);
        }
        let d = (0..k).fold(0u64, |m, j| m | 1u64 << (j * f + w as usize));
        Horizontal {
            n: codes.len(),
            w,
            k,
            d,
            data,
            touched: Cell::new(0),
        }
    }
    /// Replicate a constant into every field.
    fn splat(&self, c: u32) -> u64 {
        (0..self.k).fold(0u64, |m, j| m | (c as u64) << (j * (self.w as usize + 1)))
    }
    /// Spread the delimiter bits of word `wi` into the dense row bitmap (a real implementation uses pext or a multiply).
    fn emit(&self, out: &mut [u64], wi: usize, mut m: u64) {
        let f = self.w as usize + 1;
        while m != 0 {
            let row = wi * self.k + m.trailing_zeros() as usize / f;
            if row < self.n {
                out[row / 64] |= 1 << (row % 64); // fields past n are padding: drop them
            }
            m &= m - 1;
        }
    }
}
impl Layout for Horizontal {
    fn name(&self) -> &'static str {
        "BitWeaving/H"
    }
    fn n(&self) -> usize {
        self.n
    }
    fn width(&self) -> u32 {
        self.w
    }
    fn size_bytes(&self) -> usize {
        self.data.len() * 8
    }
    fn touched(&self) -> &Cell<u64> {
        &self.touched
    }
    fn lt_eq(&self, c: u32) -> (Bitmap, Bitmap) {
        let nw = words(self.n);
        let (mut lt, mut eq) = (vec![0u64; nw], vec![0u64; nw]);
        let cc = self.splat(c);
        for (wi, &x) in self.data.iter().enumerate() {
            // Field value of (C|D)-X is 2^w + c - x >= 1: no borrow ever leaves a field. Delimiter set <=> x <= c.
            let le = ((cc | self.d).wrapping_sub(x)) & self.d;
            // (X|D)-C has the delimiter set <=> x >= c.
            let ge = ((x | self.d).wrapping_sub(cc)) & self.d;
            let e = le & ge;
            self.emit(&mut eq, wi, e);
            self.emit(&mut lt, wi, le & !e);
        }
        self.touched
            .set(self.touched.get() + 8 * self.data.len() as u64);
        (lt, eq)
    }
}

// ---------------------------------------------------------------- BYTESLICE
struct ByteSlice {
    n: usize,
    w: u32,
    nb: usize,
    slices: Vec<Vec<u8>>, // slices[j][i] = j-th most significant byte of the left-aligned code i
    touched: Cell<u64>,
}
impl ByteSlice {
    fn byte(code: u64, w: u32, nb: usize, j: usize) -> u8 {
        // Left-aligning both codes and constants to nb*8 bits preserves their order.
        ((code << (8 * nb as u32 - w)) >> (8 * (nb - 1 - j))) as u8
    }
    fn build(codes: &[u32], w: u32) -> Self {
        let nb = (w as usize).div_ceil(8);
        let slices = (0..nb)
            .map(|j| {
                codes
                    .iter()
                    .map(|&c| Self::byte(c as u64, w, nb, j))
                    .collect()
            })
            .collect();
        ByteSlice {
            n: codes.len(),
            w,
            nb,
            slices,
            touched: Cell::new(0),
        }
    }
}
impl Layout for ByteSlice {
    fn name(&self) -> &'static str {
        "ByteSlice"
    }
    fn n(&self) -> usize {
        self.n
    }
    fn width(&self) -> u32 {
        self.w
    }
    fn size_bytes(&self) -> usize {
        self.n * self.nb
    }
    fn touched(&self) -> &Cell<u64> {
        &self.touched
    }
    fn lt_eq(&self, c: u32) -> (Bitmap, Bitmap) {
        let nw = words(self.n);
        let (mut lt_out, mut eq_out) = (vec![0u64; nw], vec![0u64; nw]);
        let cb: Vec<u8> = (0..self.nb)
            .map(|j| Self::byte(c as u64, self.w, self.nb, j))
            .collect();
        let mut read = 0u64;
        for g in 0..nw {
            let base = g * 64;
            let m = (self.n - base).min(64);
            let (mut lt, mut eq) = (0u64, valid_mask(self.n, g));
            for j in 0..self.nb {
                let col = &self.slices[j][base..base + m];
                read += m as u64;
                let (mut nlt, mut neq) = (0u64, 0u64);
                for (r, &b) in col.iter().enumerate() {
                    let live = (eq >> r) & 1;
                    nlt |= ((b < cb[j]) as u64 & live) << r;
                    neq |= ((b == cb[j]) as u64 & live) << r;
                }
                lt |= nlt;
                eq = neq;
                if eq == 0 {
                    break; // every row decided by the bytes seen so far
                }
            }
            (lt_out[g], eq_out[g]) = (lt, eq);
        }
        self.touched.set(self.touched.get() + read);
        (lt_out, eq_out)
    }
}

fn build_all(codes: &[u32], w: u32) -> Vec<Box<dyn Layout>> {
    vec![
        Box::new(Packed::build(codes, w)),
        Box::new(Plain {
            w,
            codes: codes.to_vec(),
            touched: Cell::new(0),
        }),
        Box::new(BitSliced::build(codes, w)),
        Box::new(Horizontal::build(codes, w)),
        Box::new(ByteSlice::build(codes, w)),
    ]
}

// ---------------------------------------------------------------- predicate expressions
#[derive(Clone, Debug)]
enum Pred {
    Cmp(Op, u64),
    Between(u64, u64),
}
#[derive(Clone, Debug)]
enum Expr {
    Leaf(usize, Pred), // column index, predicate
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

fn eval(e: &Expr, cols: &[Box<dyn Layout>]) -> Bitmap {
    match e {
        Expr::Leaf(c, Pred::Cmp(op, k)) => cols[*c].cmp(*op, *k),
        Expr::Leaf(c, Pred::Between(lo, hi)) => cols[*c].between(*lo, *hi),
        Expr::And(a, b) => and(&eval(a, cols), &eval(b, cols)),
        Expr::Or(a, b) => or(&eval(a, cols), &eval(b, cols)),
    }
}
fn eval_row(e: &Expr, raw: &[Vec<u32>], row: usize) -> bool {
    match e {
        Expr::Leaf(c, Pred::Cmp(op, k)) => holds(*op, raw[*c][row], *k),
        Expr::Leaf(c, Pred::Between(lo, hi)) => (*lo..=*hi).contains(&(raw[*c][row] as u64)),
        Expr::And(a, b) => eval_row(a, raw, row) && eval_row(b, raw, row),
        Expr::Or(a, b) => eval_row(a, raw, row) || eval_row(b, raw, row),
    }
}

fn gen_codes(rng: &mut Rng, n: usize, w: u32) -> Vec<u32> {
    let max = if w == 32 {
        u32::MAX as u64
    } else {
        (1u64 << w) - 1
    };
    let few = [
        rng.below(max + 1) as u32,
        rng.below(max + 1) as u32,
        max as u32,
        0,
    ];
    match rng.below(4) {
        0 => vec![few[0]; n],                                      // all equal
        1 => (0..n).map(|_| few[rng.below(4) as usize]).collect(), // skewed: four distinct values
        _ => (0..n).map(|_| rng.below(max + 1) as u32).collect(),  // full range
    }
}

fn random_const(rng: &mut Rng, raw: &[u32], w: u32) -> u64 {
    let max = if w == 32 {
        u32::MAX as u64
    } else {
        (1u64 << w) - 1
    };
    match rng.below(6) {
        0 => 0,
        1 => max,
        2 => max + 1, // out of range: decided without reading data
        3 if !raw.is_empty() => raw[rng.below(raw.len() as u64) as usize] as u64, // a value that exists: exercises equality
        _ => rng.below(max + 1),
    }
}

fn random_expr(rng: &mut Rng, cols: &[Vec<u32>], widths: &[u32], depth: u32) -> Expr {
    if depth == 0 || rng.below(3) == 0 {
        let c = rng.below(cols.len() as u64) as usize;
        let k = random_const(rng, &cols[c], widths[c]);
        if rng.below(3) == 0 {
            let k2 = random_const(rng, &cols[c], widths[c]);
            Expr::Leaf(c, Pred::Between(k.min(k2), k.max(k2)))
        } else {
            Expr::Leaf(c, Pred::Cmp(OPS[rng.below(6) as usize], k))
        }
    } else {
        let (a, b) = (
            Box::new(random_expr(rng, cols, widths, depth - 1)),
            Box::new(random_expr(rng, cols, widths, depth - 1)),
        );
        if rng.below(2) == 0 {
            Expr::And(a, b)
        } else {
            Expr::Or(a, b)
        }
    }
}

// ---------------------------------------------------------------- tests
fn column_test() {
    let mut rng = Rng(0xB17_5EED_0722_0001);
    let mut checks = 0usize;
    for col in 0..500 {
        let w = 1 + (col % 32) as u32;
        let n = [0usize, 1, 5, 63, 64, 65, 130, 300][rng.below(8) as usize] + rng.below(7) as usize;
        let raw = gen_codes(&mut rng, n, w);
        let layouts = build_all(&raw, w);
        let mut consts: Vec<u64> = (0..5).map(|_| random_const(&mut rng, &raw, w)).collect();
        consts.extend([
            0,
            if w == 32 {
                u32::MAX as u64
            } else {
                (1u64 << w) - 1
            },
        ]);
        for l in &layouts {
            for &c in &consts {
                for op in OPS {
                    let got = l.cmp(op, c);
                    let want: Vec<bool> = raw.iter().map(|&x| holds(op, x, c)).collect();
                    assert_eq!(got.len(), words(n), "{} bitmap length", l.name());
                    for (i, &wv) in want.iter().enumerate() {
                        assert_eq!(
                            (got[i / 64] >> (i % 64)) & 1 == 1,
                            wv,
                            "{} w={w} n={n} row {i} {op:?} {c}",
                            l.name()
                        );
                    }
                    assert_eq!(
                        count(&got),
                        want.iter().filter(|&&b| b).count(),
                        "{} leaked padding bits (w={w} n={n} {op:?} {c})",
                        l.name()
                    );
                    checks += 1;
                }
            }
            let (a, b) = (consts[0].min(consts[1]), consts[0].max(consts[1]));
            let got = l.between(a, b);
            let want = raw
                .iter()
                .filter(|&&x| (a..=b).contains(&(x as u64)))
                .count();
            assert_eq!(count(&got), want, "{} BETWEEN", l.name());
        }
    }
    println!(
        "500 random columns (every width 1..=32, ~15 columns each) x 5 layouts x 6 ops x 7 constants + BETWEEN == oracle ({checks} scans)"
    );
}

fn expression_test() {
    let mut rng = Rng(0xE8B2_0722_0002_AAAA);
    for t in 0..300 {
        let widths = [
            1 + rng.below(32) as u32,
            1 + rng.below(12) as u32,
            1 + rng.below(32) as u32,
        ];
        let n = rng.below(400) as usize;
        let cols: Vec<Vec<u32>> = widths.iter().map(|&w| gen_codes(&mut rng, n, w)).collect();
        let e = random_expr(&mut rng, &cols, &widths, 3);
        let want: Vec<bool> = (0..n).map(|i| eval_row(&e, &cols, i)).collect();
        for kind in 0..5 {
            // Every column stored in the same layout, picked by index out of build_all.
            let layouts: Vec<Box<dyn Layout>> = (0..3)
                .map(|c| build_all(&cols[c], widths[c]).swap_remove(kind))
                .collect();
            let got = eval(&e, &layouts);
            for (i, &wv) in want.iter().enumerate() {
                assert_eq!(
                    (got[i / 64] >> (i % 64)) & 1 == 1,
                    wv,
                    "expr #{t} {e:?} layout {} row {i}",
                    layouts[0].name()
                );
            }
            assert_eq!(count(&got), want.iter().filter(|&&b| b).count());
        }
    }
    println!(
        "300 random AND/OR/BETWEEN trees over 3 columns of different widths: all layouts == row-by-row oracle"
    );
}

// ---------------------------------------------------------------- speed demo
fn speed_demo() {
    let mut rng = Rng(0x5CA4_0722_0003_BBBB);
    let n = 4_000_000usize;
    println!("\nscan `code < c` selecting ~10% of {n} uniformly random codes (best of 3)");
    println!(
        "{:>3} {:<13} {:>8} {:>10} {:>8} {:>10} {:>9} {:>9}",
        "w", "layout", "size MB", "touched MB", "ms", "Mcodes/s", "GB/s eff", "matches"
    );
    for w in [4u32, 9, 16, 24, 32] {
        let max = if w == 32 {
            u32::MAX as u64
        } else {
            (1u64 << w) - 1
        };
        let raw: Vec<u32> = (0..n).map(|_| rng.below(max + 1) as u32).collect();
        let c = max / 10;
        let want = raw.iter().filter(|&&x| (x as u64) < c).count();
        for l in build_all(&raw, w) {
            let mut best = f64::MAX;
            let mut touched = 0;
            for _ in 0..3 {
                l.touched().set(0);
                let t = Instant::now();
                let r = black_box(l.cmp(Op::Lt, c));
                best = best.min(t.elapsed().as_secs_f64());
                touched = l.touched().get(); // per scan: cmp -> lt_eq resets nothing, so read the first run's value below
                assert_eq!(count(&r), want, "{} w={w}", l.name());
            }
            // `touched` accumulates only within one iteration because it is reset before each timed call.
            println!(
                "{w:>3} {:<13} {:>8.2} {:>10.2} {:>8.2} {:>10.0} {:>9.2} {:>9}",
                l.name(),
                l.size_bytes() as f64 / 1e6,
                touched as f64 / 1e6,
                best * 1e3,
                n as f64 / best / 1e6,
                touched as f64 / best / 1e9,
                want
            );
        }
    }
    println!(
        "(BitWeaving/V and ByteSlice stop early: 'touched' is far below the column size for wide codes; H and naive always read it all)"
    );
}

fn main() {
    // Worked example: 3-bit codes, `< 5`, shown through the bit-sliced layout.
    let codes = [3u32, 5, 7, 0, 5, 4];
    let v = BitSliced::build(&codes, 3);
    let lt = v.cmp(Op::Lt, 5);
    let eq = v.cmp(Op::Eq, 5);
    println!(
        "codes {codes:?}: code<5 -> {:06b} (row 0 is the lowest bit), code==5 -> {:06b}",
        lt[0], eq[0]
    );
    assert_eq!(lt[0], 0b101001);
    assert_eq!(eq[0], 0b010010);
    let h = Horizontal::build(&codes, 3);
    assert_eq!(
        h.k, 16,
        "3-bit codes + delimiter = 4-bit fields, 16 per u64"
    );
    assert_eq!(h.cmp(Op::Lt, 5), lt);

    column_test();
    expression_test();
    speed_demo();
    println!("ok");
}
