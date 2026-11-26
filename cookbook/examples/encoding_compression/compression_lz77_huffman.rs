//! RECIPE 183 (compression): a mini DEFLATE: LZ77 hash-chain matcher + canonical Huffman + CRC32 framing.
//! LEVEL: advanced
//! Use in: file/wire formats, HTTP bodies, archives, log shipping.   Run: cargo run --release --example compression_lz77_huffman
//!
//! WHAT: a compressor in two stages, like gzip/zlib. Stage 1 (LZ77) removes REDUNDANCY: repeated byte strings
//! become (length, distance) back-references into a 32 KiB sliding window. Stage 2 (Huffman) removes SKEWED
//! SYMBOL FREQUENCIES: common symbols get short bit codes. Entropy is the floor for stage 2 (order-0 Shannon
//! entropy, bits/byte); redundancy (repeats, long-range structure) is what stage 1 exploits and entropy can't see.
//! Random bytes have no redundancy and maximal entropy, so nothing helps: we fall back to a STORED block.
//!
//! HOW: (1) a hash chain over 3-byte prefixes finds earlier matches (`head[h]` = newest position with hash h,
//! `prev[i]` = older position with the same hash); we walk at most MAX_CHAIN links, trading ratio for speed.
//! (2) Lazy matching: if the match at i+1 is longer, emit a literal now. (3) Symbols: 0..=255 literals, 256 =
//! end-of-block, 257.. = match lengths 3..=258; distances use 30 log-spaced buckets plus raw extra bits.
//! (4) Code lengths come from a Huffman tree built with a BinaryHeap, capped at 15 bits by flattening the
//! frequencies; codes are CANONICAL so only the length table is stored. (5) Frame: magic, mode, length, CRC32.
//! The decoder treats input as hostile: every read is bounds-checked, output size is capped by the header,
//! and every loop consumes bits, so corrupt data returns `Err` and never panics or spins.
//!
//! COMMON USES: gzip/zlib/PNG-style DEFLATE streams, HTTP Content-Encoding, backup/archive tools, columnar
//! storage and log blocks (usually LZ4/zstd-class codecs, same LZ + entropy-coder shape), RLE for fax-like
//! or sparse bitmaps. Real code: use the `flate2` crate or `zstd`.
//! Read: https://www.rfc-editor.org/rfc/rfc1951  (DEFLATE)   https://docs.rs/flate2
#![allow(dead_code)]
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const WINDOW: usize = 32768;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const HASH_BITS: u32 = 15;
const MAX_CHAIN: usize = 64;
const EOB: usize = 256;
const LEN_BASE: usize = 257;
const NUM_LL: usize = LEN_BASE + (MAX_MATCH - MIN_MATCH + 1); // 513
const NUM_DIST: usize = 30;
const MAX_CODE_LEN: usize = 15;
const NONE: u32 = u32::MAX;
const MAGIC: &[u8; 4] = b"LZH1";
const HEADER_LEN: usize = 13;
const MAX_OUTPUT: usize = 1 << 28; // refuse "zip bombs" declared in the header

#[derive(Debug, PartialEq)]
enum Error {
    BadMagic,
    Truncated,
    BadHeader,
    BadCode,
    BadDistance,
    LengthMismatch,
    ChecksumMismatch,
    TooLarge,
}

// ---------- CRC32 (IEEE, reflected, polynomial 0xEDB88320) ----------
fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    !data.iter().fold(!0u32, |c, &b| {
        table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8)
    })
}

// ---------- bit IO (MSB-first so Huffman codes are written as-is) ----------
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    cur: u8,
    nbits: u32,
}
impl BitWriter {
    fn put(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            self.cur = (self.cur << 1) | ((value >> i) & 1) as u8;
            self.nbits += 1;
            if self.nbits == 8 {
                self.bytes.push(self.cur);
                self.cur = 0;
                self.nbits = 0;
            }
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.nbits > 0 {
            self.bytes.push(self.cur << (8 - self.nbits));
        }
        self.bytes
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize, // bit position
}
impl<'a> BitReader<'a> {
    fn bit(&mut self) -> Result<u32, Error> {
        let byte = *self.data.get(self.pos / 8).ok_or(Error::Truncated)?;
        let b = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Ok(b as u32)
    }
    fn get(&mut self, n: u32) -> Result<u32, Error> {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }
}

// ---------- LZ77 ----------
#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Lit(u8),
    Match { len: usize, dist: usize },
}

struct MatchFinder<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
}
impl<'a> MatchFinder<'a> {
    fn new(data: &'a [u8]) -> Self {
        MatchFinder {
            data,
            head: vec![NONE; 1 << HASH_BITS],
            prev: vec![NONE; data.len()],
        }
    }
    fn hash(&self, i: usize) -> usize {
        let d = self.data;
        let v = (d[i] as u32) << 16 | (d[i + 1] as u32) << 8 | d[i + 2] as u32;
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    }
    /// Register position `i` as a future match candidate. Call AFTER `find(i)`.
    fn insert(&mut self, i: usize) {
        if i + MIN_MATCH <= self.data.len() {
            let h = self.hash(i);
            self.prev[i] = self.head[h];
            self.head[h] = i as u32;
        }
    }
    /// Longest earlier match for position `i` within the window (walks the hash chain, newest first).
    fn find(&self, i: usize) -> Option<(usize, usize)> {
        let n = self.data.len();
        if i + MIN_MATCH > n {
            return None;
        }
        let max_len = MAX_MATCH.min(n - i);
        let (mut best_len, mut best_dist) = (MIN_MATCH - 1, 0);
        let mut cand = self.head[self.hash(i)];
        let mut steps = 0;
        while cand != NONE && steps < MAX_CHAIN {
            let c = cand as usize;
            let dist = i - c;
            if dist > WINDOW {
                break; // chain only gets older from here
            }
            let l = (0..max_len)
                .take_while(|&k| self.data[c + k] == self.data[i + k])
                .count();
            if l > best_len {
                (best_len, best_dist) = (l, dist);
                if l == max_len {
                    break;
                }
            }
            cand = self.prev[c];
            steps += 1;
        }
        (best_len >= MIN_MATCH).then_some((best_len, best_dist))
    }
}

fn tokenize(data: &[u8]) -> Vec<Token> {
    let mut mf = MatchFinder::new(data);
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let Some((len, dist)) = mf.find(i) else {
            mf.insert(i);
            out.push(Token::Lit(data[i]));
            i += 1;
            continue;
        };
        mf.insert(i);
        // Lazy matching: a longer match starting one byte later beats this one.
        if len < MAX_MATCH && mf.find(i + 1).is_some_and(|(l2, _)| l2 > len) {
            out.push(Token::Lit(data[i]));
            i += 1;
            continue;
        }
        out.push(Token::Match { len, dist });
        for k in 1..len {
            mf.insert(i + k);
        }
        i += len;
    }
    out
}

fn untokenize(tokens: &[Token]) -> Vec<u8> {
    let mut out = Vec::new();
    for &t in tokens {
        match t {
            Token::Lit(b) => out.push(b),
            Token::Match { len, dist } => copy_match(&mut out, len, dist),
        }
    }
    out
}

/// Byte-by-byte on purpose: when dist < len the match overlaps its own output (that is how "aaaa" is encoded).
fn copy_match(out: &mut Vec<u8>, len: usize, dist: usize) {
    let start = out.len() - dist;
    for k in 0..len {
        out.push(out[start + k]);
    }
}

// ---------- distance buckets: log-spaced, DEFLATE-like ----------
/// d1 = dist - 1. Returns (bucket symbol, number of extra bits, extra bits value).
fn dist_code(d1: usize) -> (usize, u32, u32) {
    if d1 < 2 {
        return (d1, 0, 0);
    }
    let nb = usize::BITS - 1 - d1.leading_zeros();
    let code = 2 * nb as usize + ((d1 >> (nb - 1)) & 1);
    let extra = nb - 1;
    (code, extra, (d1 & ((1 << extra) - 1)) as u32)
}
fn dist_from_code(code: usize, r: &mut BitReader) -> Result<usize, Error> {
    if code < 2 {
        return Ok(code + 1);
    }
    let nb = (code / 2) as u32;
    let extra = r.get(nb - 1)? as usize;
    Ok(((1 << nb) | ((code & 1) << (nb - 1)) | extra) + 1)
}

// ---------- Huffman ----------
/// Optimal prefix-code lengths via the classic greedy merge with a min-heap.
fn huffman_lengths_unbounded(freqs: &[u64]) -> Vec<u8> {
    let n = freqs.len();
    let mut lens = vec![0u8; n];
    let mut heap = BinaryHeap::new();
    for (s, &f) in freqs.iter().enumerate() {
        if f > 0 {
            heap.push(Reverse((f, s))); // (freq, node id): ties broken deterministically
        }
    }
    if heap.len() == 1 {
        let Reverse((_, s)) = heap.pop().unwrap();
        lens[s] = 1; // a lone symbol still needs one bit
        return lens;
    }
    let mut children: Vec<(usize, usize)> = Vec::new(); // internal node id = n + index
    while heap.len() > 1 {
        let Reverse((fa, a)) = heap.pop().unwrap();
        let Reverse((fb, b)) = heap.pop().unwrap();
        children.push((a, b));
        heap.push(Reverse((fa + fb, n + children.len() - 1)));
    }
    if let Some(Reverse((_, root))) = heap.pop() {
        let mut stack = vec![(root, 0u32)];
        while let Some((node, depth)) = stack.pop() {
            if node < n {
                lens[node] = depth as u8;
            } else {
                let (a, b) = children[node - n];
                stack.push((a, depth + 1));
                stack.push((b, depth + 1));
            }
        }
    }
    lens
}

/// Cap code lengths: when the optimal tree is too deep, halve the frequencies (floor 1) and retry.
/// This flattens the distribution, so it terminates (all-ones gives a balanced tree).
fn huffman_lengths(freqs: &[u64], max_len: usize) -> Vec<u8> {
    let mut f = freqs.to_vec();
    loop {
        let lens = huffman_lengths_unbounded(&f);
        if lens.iter().all(|&l| l as usize <= max_len) {
            return lens;
        }
        for x in f.iter_mut().filter(|x| **x > 0) {
            *x = (*x / 2).max(1);
        }
    }
}

/// Canonical codes: shorter codes first, ties by symbol value. Both sides rebuild them from lengths alone.
fn canonical_codes(lens: &[u8]) -> Vec<u32> {
    let mut count = [0u32; MAX_CODE_LEN + 1];
    for &l in lens.iter().filter(|&&l| l > 0) {
        count[l as usize] += 1;
    }
    let mut next = [0u32; MAX_CODE_LEN + 2];
    for l in 1..=MAX_CODE_LEN {
        next[l] = (next[l - 1] + count[l - 1]) << 1;
    }
    lens.iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            c
        })
        .collect()
}

struct HuffDecoder {
    count: [u32; MAX_CODE_LEN + 1],
    symbols: Vec<u16>, // sorted by (length, symbol)
}
impl HuffDecoder {
    fn new(lens: &[u8]) -> Result<Self, Error> {
        let mut count = [0u32; MAX_CODE_LEN + 1];
        for &l in lens.iter().filter(|&&l| l > 0) {
            count[l as usize] += 1;
        }
        let mut left = 1i64; // Kraft check: reject over-subscribed (impossible) code sets
        for l in 1..=MAX_CODE_LEN {
            left = (left << 1) - count[l] as i64;
            if left < 0 {
                return Err(Error::BadCode);
            }
        }
        let mut symbols = Vec::new();
        for l in 1..=MAX_CODE_LEN as u8 {
            symbols.extend(
                lens.iter()
                    .enumerate()
                    .filter(|(_, x)| **x == l)
                    .map(|(s, _)| s as u16),
            );
        }
        Ok(HuffDecoder { count, symbols })
    }
    /// Read one bit at a time, tracking the first canonical code of each length.
    fn decode(&self, r: &mut BitReader) -> Result<usize, Error> {
        let (mut code, mut first, mut index) = (0i64, 0i64, 0i64);
        for l in 1..=MAX_CODE_LEN {
            code |= r.bit()? as i64;
            let c = self.count[l] as i64;
            if code - first < c {
                return Ok(self.symbols[(index + code - first) as usize] as usize);
            }
            index += c;
            first = (first + c) << 1;
            code <<= 1;
        }
        Err(Error::BadCode)
    }
}

// ---------- container ----------
fn frame(mode: u8, orig: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(mode);
    out.extend_from_slice(&(orig.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc32(orig).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn compress(data: &[u8]) -> Vec<u8> {
    let tokens = tokenize(data);
    let mut ll_freq = vec![0u64; NUM_LL];
    let mut d_freq = vec![0u64; NUM_DIST];
    ll_freq[EOB] = 1;
    for t in &tokens {
        match *t {
            Token::Lit(b) => ll_freq[b as usize] += 1,
            Token::Match { len, dist } => {
                ll_freq[LEN_BASE + len - MIN_MATCH] += 1;
                d_freq[dist_code(dist - 1).0] += 1;
            }
        }
    }
    let ll_len = huffman_lengths(&ll_freq, MAX_CODE_LEN);
    let d_len = huffman_lengths(&d_freq, MAX_CODE_LEN);
    let (ll_code, d_code) = (canonical_codes(&ll_len), canonical_codes(&d_len));

    let mut w = BitWriter::default();
    for &l in ll_len.iter().chain(&d_len) {
        w.put(l as u32, 4); // the whole "dictionary" is just this length table
    }
    for t in &tokens {
        match *t {
            Token::Lit(b) => w.put(ll_code[b as usize], ll_len[b as usize] as u32),
            Token::Match { len, dist } => {
                let s = LEN_BASE + len - MIN_MATCH;
                w.put(ll_code[s], ll_len[s] as u32);
                let (dc, nbits, extra) = dist_code(dist - 1);
                w.put(d_code[dc], d_len[dc] as u32);
                w.put(extra, nbits);
            }
        }
    }
    w.put(ll_code[EOB], ll_len[EOB] as u32);
    let payload = w.finish();
    if payload.len() >= data.len() {
        return frame(0, data, data); // incompressible: stored mode costs only the header
    }
    frame(1, data, &payload)
}

fn decompress(buf: &[u8]) -> Result<Vec<u8>, Error> {
    if buf.len() < HEADER_LEN {
        return Err(Error::Truncated);
    }
    if &buf[..4] != MAGIC {
        return Err(Error::BadMagic);
    }
    let mode = buf[4];
    let orig_len = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    let crc = u32::from_le_bytes(buf[9..13].try_into().unwrap());
    if orig_len > MAX_OUTPUT {
        return Err(Error::TooLarge);
    }
    let body = &buf[HEADER_LEN..];
    let out = match mode {
        0 if body.len() == orig_len => body.to_vec(),
        0 => return Err(Error::LengthMismatch),
        1 => inflate(body, orig_len)?,
        _ => return Err(Error::BadHeader),
    };
    if out.len() != orig_len {
        return Err(Error::LengthMismatch);
    }
    if crc32(&out) != crc {
        return Err(Error::ChecksumMismatch);
    }
    Ok(out)
}

fn inflate(body: &[u8], orig_len: usize) -> Result<Vec<u8>, Error> {
    let mut r = BitReader { data: body, pos: 0 };
    let mut lens = Vec::with_capacity(NUM_LL + NUM_DIST);
    for _ in 0..NUM_LL + NUM_DIST {
        lens.push(r.get(4)? as u8);
    }
    let ll = HuffDecoder::new(&lens[..NUM_LL])?;
    let dd = HuffDecoder::new(&lens[NUM_LL..])?;
    let mut out = Vec::with_capacity(orig_len.min(1 << 20));
    loop {
        let sym = ll.decode(&mut r)?;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == EOB {
            return Ok(out);
        } else {
            let len = sym - LEN_BASE + MIN_MATCH;
            let dcode = dd.decode(&mut r)?;
            let dist = dist_from_code(dcode, &mut r)?;
            if dist > out.len() {
                return Err(Error::BadDistance); // points before the start of output
            }
            copy_match(&mut out, len, dist);
        }
        if out.len() > orig_len {
            return Err(Error::LengthMismatch); // bounds output even for a malicious stream
        }
    }
}

// ---------- RLE baseline ----------
fn rle_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        let run = data[i..].iter().take(255).take_while(|&&x| x == b).count();
        out.extend_from_slice(&[run as u8, b]);
        i += run;
    }
    out
}
fn rle_decode(buf: &[u8]) -> Result<Vec<u8>, Error> {
    if buf.len() % 2 != 0 {
        return Err(Error::Truncated);
    }
    let mut out = Vec::new();
    for pair in buf.chunks(2) {
        if pair[0] == 0 {
            return Err(Error::BadHeader);
        }
        out.extend(std::iter::repeat_n(pair[1], pair[0] as usize));
    }
    Ok(out)
}

// ---------- test support ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn random_bytes(rng: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| rng.next() as u8).collect()
}

fn text_like(rng: &mut Rng, words: usize) -> Vec<u8> {
    const W: [&str; 24] = [
        "the", "of", "and", "stream", "token", "match", "window", "to", "in", "is", "compress",
        "huffman", "code", "length", "distance", "a", "that", "with", "for", "data", "entropy",
        "bit", "table", "symbol",
    ];
    let mut s = String::new();
    for _ in 0..words {
        // Skew toward early words so symbol frequencies are uneven (that is what Huffman feeds on).
        let i = (rng.below(24) * rng.below(24) / 24) as usize;
        s.push_str(W[i]);
        s.push(' ');
    }
    s.into_bytes()
}

/// Order-0 Shannon entropy in bits per byte: the best any per-symbol code can do.
fn entropy(data: &[u8]) -> f64 {
    let mut freq = [0usize; 256];
    data.iter().for_each(|&b| freq[b as usize] += 1);
    let n = data.len() as f64;
    freq.iter()
        .filter(|&&f| f > 0)
        .map(|&f| -(f as f64 / n) * (f as f64 / n).log2())
        .sum()
}

fn roundtrip(data: &[u8]) -> usize {
    let c = compress(data);
    assert_eq!(
        decompress(&c).as_deref(),
        Ok(data),
        "round trip failed for len {}",
        data.len()
    );
    assert_eq!(rle_decode(&rle_encode(data)).as_deref(), Ok(data));
    c.len()
}

fn main() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926); // the standard CRC-32 check value
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);

    // Edge cases.
    assert_eq!(roundtrip(b""), HEADER_LEN + 0);
    roundtrip(b"x");
    roundtrip(&vec![0u8; 100_000]);
    roundtrip(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let zeros = compress(&vec![0u8; 100_000]);
    assert!(
        zeros.len() < 700,
        "all-zero input should collapse: {}",
        zeros.len()
    );

    // Tokenizer alone must be lossless, and overlapping matches must occur on run-heavy input.
    let sample = b"abcabcabcabcabc-abcabcabcabcabc";
    let toks = tokenize(sample);
    assert_eq!(untokenize(&toks), sample);
    assert!(
        toks.iter()
            .any(|t| matches!(t, Token::Match { len, dist } if dist < len))
    );

    // Length-limited Huffman: Fibonacci frequencies force a very deep optimal tree.
    let mut fib = vec![1u64, 1];
    for i in 2..40 {
        fib.push(fib[i - 1] + fib[i - 2]);
    }
    assert!(
        huffman_lengths_unbounded(&fib)
            .iter()
            .any(|&l| l as usize > MAX_CODE_LEN)
    );
    let lens = huffman_lengths(&fib, MAX_CODE_LEN);
    assert!(lens.iter().all(|&l| l as usize <= MAX_CODE_LEN && l > 0));
    let kraft: f64 = lens.iter().map(|&l| 0.5f64.powi(l as i32)).sum();
    assert!(kraft <= 1.0 + 1e-12);

    // Many random shapes: random, repetitive, mixed, tiny.
    for round in 0..60 {
        let n = (rng.below(3000) + round) as usize;
        roundtrip(&random_bytes(&mut rng, n));
        let alphabet = rng.below(4) + 1;
        let v: Vec<u8> = (0..n).map(|_| b'a' + rng.below(alphabet) as u8).collect();
        roundtrip(&v);
        let mut mixed = text_like(&mut rng, 40);
        mixed.extend(random_bytes(&mut rng, n % 50));
        mixed.extend_from_within(..mixed.len() / 2);
        roundtrip(&mixed);
    }
    // Long input crossing the 32 KiB window boundary.
    let mut long = text_like(&mut rng, 20_000);
    long.extend(random_bytes(&mut rng, 5_000));
    long.extend(text_like(&mut rng, 5_000));
    roundtrip(&long);

    // Hostile input: every truncation fails; every single-bit flip fails or (impossibly) restores the input.
    let victim_src = text_like(&mut rng, 300);
    let victim = compress(&victim_src);
    assert_eq!(victim[4], 1, "victim should be Huffman mode");
    for cut in 0..victim.len() {
        assert!(
            decompress(&victim[..cut]).is_err(),
            "truncation at {cut} accepted"
        );
    }
    for byte in 0..victim.len() {
        for bit in 0..8 {
            let mut bad = victim.clone();
            bad[byte] ^= 1 << bit;
            if let Ok(out) = decompress(&bad) {
                assert_eq!(
                    out, victim_src,
                    "corruption at {byte}:{bit} went undetected"
                );
            }
        }
    }
    // Pure garbage after a valid header must also terminate with an error, not a panic or a loop.
    for _ in 0..300 {
        let mut junk = frame(1, b"", &[]);
        junk[5..9].copy_from_slice(&(1000u32).to_le_bytes());
        junk.extend(random_bytes(&mut rng, 600));
        let _ = decompress(&junk);
    }
    assert_eq!(decompress(b"nope nope nope nope"), Err(Error::BadMagic));
    assert!(rle_decode(&[3]).is_err() && rle_decode(&[0, 7]).is_err());

    // Ratios.
    let text = text_like(&mut rng, 20_000);
    let repetitive: Vec<u8> = b"GET /index.html HTTP/1.1\r\nHost: example.org\r\n\r\n".repeat(2000);
    let random = random_bytes(&mut rng, 100_000);
    let runs: Vec<u8> = (0..100_000)
        .map(|i| b"AAAAABBBBBBBBBBBCC"[i / 7 % 18])
        .collect();
    println!(
        "{:<12} {:>8} {:>9} {:>9} {:>9} {:>8}",
        "input", "bytes", "H bits/B", "RLE", "LZ+Huff", "ratio"
    );
    for (name, data) in [
        ("text-like", &text),
        ("repetitive", &repetitive),
        ("random", &random),
        ("runs", &runs),
    ] {
        let c = roundtrip(data);
        println!(
            "{:<12} {:>8} {:>9.2} {:>9} {:>9} {:>7.1}%",
            name,
            data.len(),
            entropy(data),
            rle_encode(data).len(),
            c,
            100.0 * c as f64 / data.len() as f64
        );
    }
    assert!(roundtrip(&text) < text.len() / 2);
    assert!(roundtrip(&repetitive) < repetitive.len() / 50);
    assert!(roundtrip(&random) <= random.len() + HEADER_LEN);
    println!(
        "entropy: random ~8 bits/byte => incompressible; text < 8 => Huffman wins; repeats => LZ77 wins big."
    );
    println!("ok");
}
