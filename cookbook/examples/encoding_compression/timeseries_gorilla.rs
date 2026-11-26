//! RECIPE 860 (compression): Gorilla time-series compression: delta-of-delta timestamps, XOR floats, Chimp/Chimp128, an Elf-style eraser, simple8b integers, dictionary+RLE tags.
//! LEVEL: advanced
//! Use in: any TSDB or metrics pipeline that must keep millions of (timestamp, float) samples in RAM or in small blocks.   Run: cargo run --release --example timeseries_gorilla
//! COMMON USES: the Gorilla scheme (Facebook, 2015) is the in-memory chunk format of Prometheus/VictoriaMetrics-style engines
//! (Prometheus's XOR chunks follow it closely) and of InfluxDB's float blocks; simple8b packs the integer columns
//! of InfluxDB TSM files; dictionary + RLE is how columnar stores keep low-cardinality tag columns; Chimp and Elf are
//! 2022-23 research improvements of the XOR idea.
//! Course: CMU Database Group time-series talks (InfluxDB storage engine); paper: Gorilla (VLDB 2015), Chimp (VLDB 2022).
#![allow(dead_code)]
//!
//! WHAT: monitoring data is (timestamp, f64) pairs where consecutive timestamps are almost equally spaced and consecutive
//! values are almost equal. A raw sample is 128 bits; Gorilla reports ~1.37 bytes per sample on Facebook's data.
//! HOW (all codecs below share one MSB-first `BitWriter`/`BitReader`; chunk = header + one interleaved bit stream):
//! * TIMESTAMPS: delta-of-delta. dod = (t[i]-t[i-1]) - (t[i-1]-t[i-2]). Prefix codes: `0` (dod 0), `10`+7 bits, `110`+9,
//!   `1110`+12, `1111`+64 raw bits (the paper uses 32; 64 keeps i64 extremes exact). A perfectly regular series therefore costs
//!   ONE bit per timestamp. All arithmetic is wrapping, so even i64::MIN..MAX jumps and out-of-order input round-trip.
//! * GORILLA XOR: x = bits(v) ^ bits(prev). `0` = identical. `10` = x fits in the previous (leading, trailing) zero window:
//!   write only the window bits. `11` = new window: 5 bits leading zeros (capped at 31), 6 bits length (64 stored as 0), payload.
//! * CHIMP: observes that the window-reuse code is rarely worth it and trailing zeros only pay off when there are MANY.
//!   Flags (2 bits): `00` same, `01` trailing zeros > 6 (3-bit rounded leading, 6-bit length, payload), `10` same rounded
//!   leading as before (write 64-lead bits), `11` new rounded leading. Leading zeros are rounded DOWN to {0,8,12,16,18,20,22,24}
//!   so 3 bits suffice. Constant series cost 2 bits per value (Gorilla: 1), noisy series cost less.
//! * CHIMP128: also considers the last 128 values: a hash on the low 14 bits finds a previous value whose XOR with the
//!   current one has > 13 trailing zeros (6 + log2 128); the 7-bit ring index is the price. Wins on periodic data.
//! * ELF-STYLE (simplified): sensor values are usually decimals like 21.3. If v has d decimal places, the low mantissa bits
//!   can be ERASED (zeroed) as long as formatting the erased value to d places and parsing it returns exactly v. The erased
//!   value XORs with long trailing-zero runs; the decoder restores v by `parse(format("{:.d}"))`. We search the largest erasable
//!   bit count by binary search and VERIFY it, so the scheme is lossless by construction (the real Elf derives it arithmetically and
//!   spends fewer header bits; here 1 flag bit + 4 bits for d).
//! * INTEGERS: delta (or delta-of-delta) + zigzag + simple8b: a 64-bit word = 4-bit selector + 60 payload bits holding 240..1
//!   values; here selectors 0/1 are runs of ZEROS (InfluxDB uses ones), values >= 2^60 fall back to raw 8-byte mode.
//! * TAGS: dictionary (distinct strings once) + RLE of dictionary indexes as (index, run) varint pairs.
//! TESTS: 3000 random series (regular, jittery, bursty, constant, random walk, decimals, counters, random bits, NaN/inf/-0.0/
//! subnormals, i64 extreme timestamps, unsorted) round-trip BIT-EXACT through all four value codecs; truncation is an error, not a
//! panic; out-of-order `append` is rejected. Then a bits-per-sample table vs the raw 128 bits.
//!
//! Read: Pelkonen et al., "Gorilla: A Fast, Scalable, In-Memory Time Series Database", VLDB 2015  https://www.vldb.org/pvldb/vol8/p1816-teller.pdf
//! Read: Liakos et al., "Chimp: Efficient Lossless Floating Point Compression for Time Series Databases", VLDB 2022  https://www.vldb.org/pvldb/vol15/p3058-liakos.pdf
//! Read: Li et al., "Elf: Erasing-based Lossless Floating-Point Compression", VLDB 2023 (cited by title; link not checked)
//! Read: Anh & Moffat, "Index compression using 64-bit words" (simple8b), Software: Practice & Experience 2010 (cited by title)
//!
//! TRY NEXT:
//!   - [S] In `GorillaXor::put` change the leading-zero cap from 31 to 15 (4-bit field). Predict the change in bits/sample for
//!     `Kind::RandomWalk`, then measure with `table`; which invariant in `get` must you also change?
//!   - [S] Shrink the first-delta cost: the 68-bit escape in `TsState::put` hits sample 2. Try a 14-bit header delta like the paper
//!     and keep `test_bit_io` bucket sizes honest. What breaks for i64 extreme timestamps?
//!   - [M] Add a `Codec::Delta2Int` for series whose values are integers (counters): zigzag + simple8b on `encode_ints`. Predict whether it
//!     beats Gorilla on `Kind::Counter`, measure, and explain where Gorilla's XOR wastes bits on integer-valued floats.
//!   - [M] Failure mode: flip one random bit in a chunk and decode. Does `decode_chunk` panic, return garbage, or error? Add a CRC32 in the
//!     header and make corruption always detectable (see the `encodings_codecs` recipe for checksums).
//!   - [A] PROJECT: use `Chunk` as the block format of the `tsm_time_series_engine` recipe (swap its simple encoding), then compare
//!     on-disk bytes per sample at 1k, 100k and 1M samples per series.

use std::collections::HashMap;

// ---------------------------------------------------------------- rng
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn gauss(&mut self) -> f64 {
        (0..12).map(|_| self.f64()).sum::<f64>() - 6.0
    }
}

// ---------------------------------------------------------------- bit I/O
#[derive(Default)]
struct BitWriter {
    buf: Vec<u8>,
    nbits: usize,
}
impl BitWriter {
    fn bit(&mut self, b: bool) {
        if self.nbits % 8 == 0 {
            self.buf.push(0);
        }
        if b {
            let last = self.buf.len() - 1;
            self.buf[last] |= 0x80 >> (self.nbits % 8);
        }
        self.nbits += 1;
    }
    /// Write the low `n` bits of `v`, most significant first. n in 0..=64.
    fn bits(&mut self, v: u64, n: u32) {
        debug_assert!(n <= 64);
        for i in (0..n).rev() {
            self.bit((v >> i) & 1 == 1);
        }
    }
}
struct BitReader<'a> {
    buf: &'a [u8],
    pos: usize,
}
impl<'a> BitReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        BitReader { buf, pos: 0 }
    }
    fn bit(&mut self) -> Option<bool> {
        let byte = *self.buf.get(self.pos / 8)?;
        let b = byte & (0x80 >> (self.pos % 8)) != 0;
        self.pos += 1;
        Some(b)
    }
    fn bits(&mut self, n: u32) -> Option<u64> {
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 1) | self.bit()? as u64;
        }
        Some(v)
    }
}

// ---------------------------------------------------------------- timestamps: delta-of-delta
#[derive(Default)]
struct TsState {
    n: usize,
    prev_ts: i64,
    prev_delta: i64,
}
impl TsState {
    fn put(&mut self, w: &mut BitWriter, ts: i64) {
        if self.n == 0 {
            w.bits(ts as u64, 64);
        } else {
            let delta = ts.wrapping_sub(self.prev_ts);
            let dod = delta.wrapping_sub(self.prev_delta);
            match dod {
                0 => w.bit(false),
                -63..=64 => {
                    w.bits(0b10, 2);
                    w.bits((dod + 63) as u64, 7);
                }
                -255..=256 => {
                    w.bits(0b110, 3);
                    w.bits((dod + 255) as u64, 9);
                }
                -2047..=2048 => {
                    w.bits(0b1110, 4);
                    w.bits((dod + 2047) as u64, 12);
                }
                _ => {
                    w.bits(0b1111, 4);
                    w.bits(dod as u64, 64);
                }
            }
            self.prev_delta = delta;
        }
        self.prev_ts = ts;
        self.n += 1;
    }
    fn get(&mut self, r: &mut BitReader) -> Option<i64> {
        if self.n == 0 {
            self.prev_ts = r.bits(64)? as i64;
        } else {
            let mut ones = 0;
            while ones < 4 && r.bit()? {
                ones += 1;
            }
            let dod = match ones {
                0 => 0,
                1 => r.bits(7)? as i64 - 63,
                2 => r.bits(9)? as i64 - 255,
                3 => r.bits(12)? as i64 - 2047,
                _ => r.bits(64)? as i64,
            };
            self.prev_delta = self.prev_delta.wrapping_add(dod);
            self.prev_ts = self.prev_ts.wrapping_add(self.prev_delta);
        }
        self.n += 1;
        Some(self.prev_ts)
    }
}

// ---------------------------------------------------------------- value codecs
trait ValueCodec {
    fn put(&mut self, w: &mut BitWriter, v: f64);
    fn get(&mut self, r: &mut BitReader) -> Option<f64>;
}

#[derive(Default)]
struct GorillaXor {
    n: usize,
    prev: u64,
    window: Option<(u32, u32)>, // (leading, trailing) of the last explicit window
}
impl ValueCodec for GorillaXor {
    fn put(&mut self, w: &mut BitWriter, v: f64) {
        let bits = v.to_bits();
        if self.n == 0 {
            w.bits(bits, 64);
        } else {
            let x = bits ^ self.prev;
            if x == 0 {
                w.bit(false);
            } else {
                w.bit(true);
                let lz = x.leading_zeros().min(31); // 5 bits can hold at most 31
                let tz = x.trailing_zeros();
                match self.window {
                    Some((wl, wt)) if lz >= wl && tz >= wt => {
                        w.bit(false);
                        w.bits(x >> wt, 64 - wl - wt);
                    }
                    _ => {
                        w.bit(true);
                        let len = 64 - lz - tz;
                        w.bits(lz as u64, 5);
                        w.bits(len as u64, 6); // 64 wraps to 0, decoded back to 64
                        w.bits(x >> tz, len);
                        self.window = Some((lz, tz));
                    }
                }
            }
        }
        self.prev = bits;
        self.n += 1;
    }
    fn get(&mut self, r: &mut BitReader) -> Option<f64> {
        if self.n == 0 {
            self.prev = r.bits(64)?;
        } else if r.bit()? {
            let (lz, tz);
            if !r.bit()? {
                (lz, tz) = self.window?;
            } else {
                lz = r.bits(5)? as u32;
                let mut len = r.bits(6)? as u32;
                if len == 0 {
                    len = 64;
                }
                tz = 64u32.checked_sub(lz + len)?;
                self.window = Some((lz, tz));
            }
            let len = 64 - lz - tz;
            self.prev ^= r.bits(len)? << tz;
        }
        self.n += 1;
        Some(f64::from_bits(self.prev))
    }
}

const LEAD_ROUND: [u32; 8] = [0, 8, 12, 16, 18, 20, 22, 24];
/// Round leading zeros DOWN to a representable value; returns (3-bit code, rounded value).
fn round_lead(lz: u32) -> (u64, u32) {
    let i = LEAD_ROUND.iter().rposition(|&l| l <= lz).unwrap();
    (i as u64, LEAD_ROUND[i])
}
const NO_LEAD: u32 = 65;

struct Chimp {
    n: usize,
    prev: u64,
    stored_lead: u32,
}
impl Default for Chimp {
    fn default() -> Self {
        Chimp {
            n: 0,
            prev: 0,
            stored_lead: NO_LEAD,
        }
    }
}
impl ValueCodec for Chimp {
    fn put(&mut self, w: &mut BitWriter, v: f64) {
        let bits = v.to_bits();
        if self.n == 0 {
            w.bits(bits, 64);
        } else {
            let x = bits ^ self.prev;
            if x == 0 {
                w.bits(0b00, 2);
                self.stored_lead = NO_LEAD;
            } else {
                let (code, lr) = round_lead(x.leading_zeros());
                let tz = x.trailing_zeros();
                if tz > 6 {
                    let sig = 64 - lr - tz;
                    w.bits(0b01, 2);
                    w.bits(code, 3);
                    w.bits(sig as u64, 6);
                    w.bits(x >> tz, sig);
                    self.stored_lead = lr;
                } else if lr == self.stored_lead {
                    w.bits(0b10, 2);
                    w.bits(x, 64 - lr);
                } else {
                    w.bits(0b11, 2);
                    w.bits(code, 3);
                    w.bits(x, 64 - lr);
                    self.stored_lead = lr;
                }
            }
        }
        self.prev = bits;
        self.n += 1;
    }
    fn get(&mut self, r: &mut BitReader) -> Option<f64> {
        if self.n == 0 {
            self.prev = r.bits(64)?;
        } else {
            let x = match r.bits(2)? {
                0b00 => {
                    self.stored_lead = NO_LEAD;
                    0
                }
                0b01 => {
                    let lr = LEAD_ROUND[r.bits(3)? as usize];
                    let sig = r.bits(6)? as u32;
                    let tz = 64u32.checked_sub(lr + sig).filter(|_| sig > 0)?;
                    self.stored_lead = lr;
                    r.bits(sig)? << tz
                }
                0b10 => {
                    if self.stored_lead == NO_LEAD {
                        return None;
                    }
                    r.bits(64 - self.stored_lead)?
                }
                _ => {
                    self.stored_lead = LEAD_ROUND[r.bits(3)? as usize];
                    r.bits(64 - self.stored_lead)?
                }
            };
            self.prev ^= x;
        }
        self.n += 1;
        Some(f64::from_bits(self.prev))
    }
}

const RING: usize = 128;
const KEY_MASK: u64 = (1 << 14) - 1;
const TZ_THRESHOLD: u32 = 13; // 6 (Chimp) + 7 (index bits)

struct Chimp128 {
    n: usize,
    ring: [u64; RING],
    latest: HashMap<u64, usize>, // low 14 bits -> absolute index of the newest value with that key
    stored_lead: u32,
}
impl Default for Chimp128 {
    fn default() -> Self {
        Chimp128 {
            n: 0,
            ring: [0; RING],
            latest: HashMap::new(),
            stored_lead: NO_LEAD,
        }
    }
}
impl Chimp128 {
    fn push(&mut self, bits: u64) {
        self.ring[self.n % RING] = bits;
        self.latest.insert(bits & KEY_MASK, self.n);
        self.n += 1;
    }
}
impl ValueCodec for Chimp128 {
    fn put(&mut self, w: &mut BitWriter, v: f64) {
        let bits = v.to_bits();
        if self.n == 0 {
            w.bits(bits, 64);
            self.push(bits);
            return;
        }
        // Candidate: the newest earlier value sharing the low 14 bits (so XOR has >= 14 trailing zeros).
        let cand = self
            .latest
            .get(&(bits & KEY_MASK))
            .copied()
            .filter(|&j| self.n - j <= RING);
        if let Some(j) = cand {
            let x = bits ^ self.ring[j % RING];
            let off = (self.n - 1 - j) as u64;
            if x == 0 {
                w.bits(0b00, 2);
                w.bits(off, 7);
                self.stored_lead = NO_LEAD;
            } else {
                let (code, lr) = round_lead(x.leading_zeros());
                let tz = x.trailing_zeros();
                let sig = 64 - lr - tz;
                w.bits(0b01, 2);
                w.bits(off, 7);
                w.bits(code, 3);
                w.bits(sig as u64, 6);
                w.bits(x >> tz, sig);
                self.stored_lead = lr;
            }
        } else {
            let prev = self.ring[(self.n - 1) % RING];
            let x = bits ^ prev;
            debug_assert!(x != 0 && x.trailing_zeros() <= TZ_THRESHOLD);
            let (code, lr) = round_lead(x.leading_zeros());
            if lr == self.stored_lead {
                w.bits(0b10, 2);
                w.bits(x, 64 - lr);
            } else {
                w.bits(0b11, 2);
                w.bits(code, 3);
                w.bits(x, 64 - lr);
                self.stored_lead = lr;
            }
        }
        self.push(bits);
    }
    fn get(&mut self, r: &mut BitReader) -> Option<f64> {
        if self.n == 0 {
            let b = r.bits(64)?;
            self.push(b);
            return Some(f64::from_bits(b));
        }
        let at = |s: &Self, off: usize| (off < s.n).then(|| s.ring[(s.n - 1 - off) % RING]);
        let bits = match r.bits(2)? {
            0b00 => {
                self.stored_lead = NO_LEAD;
                at(self, r.bits(7)? as usize)?
            }
            0b01 => {
                let base = at(self, r.bits(7)? as usize)?;
                let lr = LEAD_ROUND[r.bits(3)? as usize];
                let sig = r.bits(6)? as u32;
                let tz = 64u32.checked_sub(lr + sig).filter(|_| sig > 0)?;
                self.stored_lead = lr;
                base ^ (r.bits(sig)? << tz)
            }
            0b10 => {
                if self.stored_lead == NO_LEAD {
                    return None;
                }
                at(self, 0)? ^ r.bits(64 - self.stored_lead)?
            }
            _ => {
                self.stored_lead = LEAD_ROUND[r.bits(3)? as usize];
                at(self, 0)? ^ r.bits(64 - self.stored_lead)?
            }
        };
        self.push(bits);
        Some(f64::from_bits(bits))
    }
}

/// Elf-style: erase low mantissa bits that the decimal representation makes redundant, then Gorilla-XOR the result.
#[derive(Default)]
struct ElfXor {
    inner: GorillaXor,
}
/// Returns (decimal places d, erased value) when erasing is provably lossless.
fn elf_erase(v: f64) -> Option<(u32, f64)> {
    if !v.is_finite() || v == 0.0 || v.abs() < 1e-5 || v.abs() >= 1e15 {
        return None;
    }
    let s = format!("{v}");
    let d = s.split_once('.').map_or(0, |(_, f)| f.len());
    if d > 15 {
        return None;
    }
    let restores = |k: u32| {
        let e = f64::from_bits(v.to_bits() & !((1u64 << k) - 1));
        format!("{e:.d$}")
            .parse::<f64>()
            .is_ok_and(|p| p.to_bits() == v.to_bits())
    };
    if !restores(0) {
        return None;
    }
    // Largest k with restores(k): binary search (a non-monotone miss only costs compression, never correctness).
    let (mut lo, mut hi) = (0u32, 52u32);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if restores(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    Some((d as u32, f64::from_bits(v.to_bits() & !((1u64 << lo) - 1))))
}
impl ValueCodec for ElfXor {
    fn put(&mut self, w: &mut BitWriter, v: f64) {
        match elf_erase(v) {
            Some((d, e)) => {
                w.bit(true);
                w.bits(d as u64, 4);
                self.inner.put(w, e);
            }
            None => {
                w.bit(false);
                self.inner.put(w, v);
            }
        }
    }
    fn get(&mut self, r: &mut BitReader) -> Option<f64> {
        if r.bit()? {
            let d = r.bits(4)? as usize;
            let e = self.inner.get(r)?;
            format!("{e:.d$}").parse::<f64>().ok()
        } else {
            self.inner.get(r)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Codec {
    Gorilla = 0,
    Chimp = 1,
    Chimp128 = 2,
    Elf = 3,
}
const CODECS: [Codec; 4] = [Codec::Gorilla, Codec::Chimp, Codec::Chimp128, Codec::Elf];
impl Codec {
    fn make(self) -> Box<dyn ValueCodec> {
        match self {
            Codec::Gorilla => Box::<GorillaXor>::default(),
            Codec::Chimp => Box::<Chimp>::default(),
            Codec::Chimp128 => Box::<Chimp128>::default(),
            Codec::Elf => Box::<ElfXor>::default(),
        }
    }
    fn from_id(id: u8) -> Option<Codec> {
        CODECS.get(id as usize).copied()
    }
}

// ---------------------------------------------------------------- chunk format
// header: "GRL1" | codec u8 | count u32 LE   (9 bytes)   then one MSB-first bit stream: [ts, value] per sample.
const MAGIC: &[u8; 4] = b"GRL1";

#[derive(Debug, PartialEq)]
enum AppendError {
    OutOfOrder { last: i64, got: i64 },
}
#[derive(Debug, PartialEq)]
enum DecodeError {
    BadHeader,
    Truncated,
}

struct Chunk {
    codec: Codec,
    ts: TsState,
    val: Box<dyn ValueCodec>,
    w: BitWriter,
    count: u32,
}
impl Chunk {
    fn new(codec: Codec) -> Self {
        Chunk {
            codec,
            ts: TsState::default(),
            val: codec.make(),
            w: BitWriter::default(),
            count: 0,
        }
    }
    /// Policy: a chunk is append-only in time. Out-of-order samples are REJECTED here (a real engine buffers and sorts them
    /// in a head block first, see the tsm_time_series_engine recipe); equal timestamps are allowed.
    fn append(&mut self, t: i64, v: f64) -> Result<(), AppendError> {
        if self.count > 0 && t < self.ts.prev_ts {
            return Err(AppendError::OutOfOrder {
                last: self.ts.prev_ts,
                got: t,
            });
        }
        self.append_any(t, v);
        Ok(())
    }
    /// The codec itself is order-agnostic (wrapping arithmetic); only the policy above forbids disorder.
    fn append_any(&mut self, t: i64, v: f64) {
        self.ts.put(&mut self.w, t);
        self.val.put(&mut self.w, v);
        self.count += 1;
    }
    fn finish(self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.push(self.codec as u8);
        out.extend_from_slice(&self.count.to_le_bytes());
        out.extend_from_slice(&self.w.buf);
        out
    }
}
fn decode_chunk(bytes: &[u8]) -> Result<Vec<(i64, f64)>, DecodeError> {
    if bytes.len() < 9 || &bytes[..4] != MAGIC {
        return Err(DecodeError::BadHeader);
    }
    let codec = Codec::from_id(bytes[4]).ok_or(DecodeError::BadHeader)?;
    let count = u32::from_le_bytes(bytes[5..9].try_into().unwrap()) as usize;
    // Guard against a lying count: every sample needs at least 1 bit of timestamp, so count <= 8 * payload.
    if count > 8 * (bytes.len() - 9) + 1 {
        return Err(DecodeError::Truncated);
    }
    let (mut r, mut ts, mut val) = (
        BitReader::new(&bytes[9..]),
        TsState::default(),
        codec.make(),
    );
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let t = ts.get(&mut r).ok_or(DecodeError::Truncated)?;
        let v = val.get(&mut r).ok_or(DecodeError::Truncated)?;
        out.push((t, v));
    }
    Ok(out)
}
fn encode_chunk(codec: Codec, ts: &[i64], vals: &[f64]) -> Vec<u8> {
    let mut c = Chunk::new(codec);
    for (&t, &v) in ts.iter().zip(vals) {
        c.append_any(t, v);
    }
    c.finish()
}

// per-stream bit accounting for the table
fn ts_bits(ts: &[i64]) -> usize {
    let (mut w, mut s) = (BitWriter::default(), TsState::default());
    ts.iter().for_each(|&t| s.put(&mut w, t));
    w.nbits
}
fn val_bits(codec: Codec, vals: &[f64]) -> usize {
    let (mut w, mut c) = (BitWriter::default(), codec.make());
    vals.iter().for_each(|&v| c.put(&mut w, v));
    w.nbits
}

// ---------------------------------------------------------------- simple8b integers
// (values per word, bits per value); selector 0/1 = runs of zeros (zero payload bits).
const SEL: [(usize, u32); 16] = [
    (240, 0),
    (120, 0),
    (60, 1),
    (30, 2),
    (20, 3),
    (15, 4),
    (12, 5),
    (10, 6),
    (8, 7),
    (7, 8),
    (6, 10),
    (5, 12),
    (4, 15),
    (3, 20),
    (2, 30),
    (1, 60),
];
fn simple8b_pack(vals: &[u64]) -> Option<Vec<u64>> {
    if vals.iter().any(|&v| v >= 1 << 60) {
        return None;
    }
    let (mut out, mut i) = (Vec::new(), 0);
    while i < vals.len() {
        for (s, &(n, bits)) in SEL.iter().enumerate() {
            if n > vals.len() - i {
                continue;
            }
            let chunk = &vals[i..i + n];
            let fits = if bits == 0 {
                chunk.iter().all(|&v| v == 0)
            } else {
                chunk.iter().all(|&v| v < 1 << bits)
            };
            if fits {
                let mut word = (s as u64) << 60;
                if bits > 0 {
                    for (j, &v) in chunk.iter().enumerate() {
                        word |= v << (j as u32 * bits);
                    }
                }
                out.push(word);
                i += n;
                break;
            }
        }
    }
    Some(out)
}
fn simple8b_unpack(words: &[u64], count: usize) -> Option<Vec<u64>> {
    let mut out = Vec::with_capacity(count);
    for &word in words {
        let (n, bits) = SEL[(word >> 60) as usize];
        for j in 0..n {
            out.push(if bits == 0 {
                0
            } else {
                (word >> (j as u32 * bits)) & ((1 << bits) - 1)
            });
        }
    }
    (out.len() == count).then_some(out)
}
fn zigzag(x: i64) -> u64 {
    ((x << 1) ^ (x >> 63)) as u64
}
fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}
/// `order` 1 = delta, 2 = delta-of-delta (right for timestamps). Layout: count u32 | order u8 | mode u8 | first i64 | payload.
fn encode_ints(vals: &[i64], order: u8) -> Vec<u8> {
    // Exact formulation: out[0]=vals[0]; residuals r1[i]=v[i]-v[i-1] (i>=1); order 2: r2[i]=r1[i]-r1[i-1] (i>=2), r2[1]=r1[1].
    let mut resid: Vec<i64> = Vec::new();
    if !vals.is_empty() {
        let r1: Vec<i64> = vals.windows(2).map(|p| p[1].wrapping_sub(p[0])).collect();
        resid = if order >= 2 {
            r1.iter()
                .enumerate()
                .map(|(i, &x)| if i == 0 { x } else { x.wrapping_sub(r1[i - 1]) })
                .collect()
        } else {
            r1
        };
    }
    let zz: Vec<u64> = resid.iter().map(|&x| zigzag(x)).collect();
    let mut out = (vals.len() as u32).to_le_bytes().to_vec();
    out.push(order);
    out.extend_from_slice(&vals.first().copied().unwrap_or(0).to_le_bytes());
    match simple8b_pack(&zz) {
        Some(words) => {
            out.push(0);
            words
                .iter()
                .for_each(|w| out.extend_from_slice(&w.to_le_bytes()));
        }
        None => {
            out.push(1);
            zz.iter()
                .for_each(|w| out.extend_from_slice(&w.to_le_bytes()));
        }
    }
    out
}
fn decode_ints(b: &[u8]) -> Option<Vec<i64>> {
    if b.len() < 14 {
        return None;
    }
    let count = u32::from_le_bytes(b[0..4].try_into().ok()?) as usize;
    let (order, first, mode) = (b[4], i64::from_le_bytes(b[5..13].try_into().ok()?), b[13]);
    let words: Vec<u64> = b[14..]
        .chunks_exact(8)
        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
        .collect();
    if count == 0 {
        return Some(vec![]);
    }
    let zz = if mode == 0 {
        simple8b_unpack(&words, count - 1)?
    } else {
        words
    };
    if zz.len() != count - 1 {
        return None;
    }
    let mut r: Vec<i64> = zz.into_iter().map(unzigzag).collect();
    if order >= 2 {
        for i in 1..r.len() {
            r[i] = r[i].wrapping_add(r[i - 1]);
        }
    }
    let mut out = vec![first];
    for x in r {
        out.push(out.last().unwrap().wrapping_add(x));
    }
    Some(out)
}

// ---------------------------------------------------------------- tags: dictionary + RLE
fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}
fn get_varint(b: &[u8], pos: &mut usize) -> Option<u64> {
    let (mut v, mut shift) = (0u64, 0);
    loop {
        let byte = *b.get(*pos)?;
        *pos += 1;
        v |= ((byte & 0x7f) as u64).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}
fn encode_tags(col: &[String]) -> Vec<u8> {
    let mut dict: Vec<&str> = Vec::new();
    let mut ids: HashMap<&str, u64> = HashMap::new();
    let mut runs: Vec<(u64, u64)> = Vec::new();
    for s in col {
        let id = *ids.entry(s.as_str()).or_insert_with(|| {
            dict.push(s.as_str());
            dict.len() as u64 - 1
        });
        match runs.last_mut() {
            Some((rid, n)) if *rid == id => *n += 1,
            _ => runs.push((id, 1)),
        }
    }
    let mut out = Vec::new();
    put_varint(&mut out, dict.len() as u64);
    for d in &dict {
        put_varint(&mut out, d.len() as u64);
        out.extend_from_slice(d.as_bytes());
    }
    put_varint(&mut out, runs.len() as u64);
    for (id, n) in runs {
        put_varint(&mut out, id);
        put_varint(&mut out, n);
    }
    out
}
fn decode_tags(b: &[u8]) -> Option<Vec<String>> {
    let mut pos = 0;
    let nd = get_varint(b, &mut pos)? as usize;
    let mut dict = Vec::new();
    for _ in 0..nd {
        let len = get_varint(b, &mut pos)? as usize;
        let s = std::str::from_utf8(b.get(pos..pos.checked_add(len)?)?).ok()?;
        dict.push(s.to_string());
        pos += len;
    }
    let nr = get_varint(b, &mut pos)?;
    let mut out = Vec::new();
    for _ in 0..nr {
        let (id, n) = (get_varint(b, &mut pos)? as usize, get_varint(b, &mut pos)?);
        out.extend(std::iter::repeat_n(dict.get(id)?.clone(), n as usize));
    }
    Some(out)
}

// ---------------------------------------------------------------- workload generators
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Regular,
    Jittery,
    Bursty,
    Constant,
    RandomWalk,
    Decimals,
    Counter,
    RandomBits,
    Special,
    Extremes,
    Unsorted,
}
const KINDS: [Kind; 11] = [
    Kind::Regular,
    Kind::Jittery,
    Kind::Bursty,
    Kind::Constant,
    Kind::RandomWalk,
    Kind::Decimals,
    Kind::Counter,
    Kind::RandomBits,
    Kind::Special,
    Kind::Extremes,
    Kind::Unsorted,
];
const SPECIALS: [f64; 12] = [
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    -0.0,
    0.0,
    f64::MIN_POSITIVE,
    5e-324,
    f64::MAX,
    f64::MIN,
    f64::EPSILON,
    1.0,
    -1.0,
];

fn gen_series(kind: Kind, n: usize, rng: &mut Rng) -> (Vec<i64>, Vec<f64>) {
    let t0 = 1_700_000_000_000 + rng.range(0, 1_000_000);
    let step = [1000, 10_000, 15_000, 60_000][rng.below(4) as usize];
    let ts: Vec<i64> = match kind {
        Kind::Jittery => (0..n as i64)
            .map(|i| t0 + i * step + rng.range(-20, 20))
            .collect(),
        Kind::Bursty => {
            let mut t = t0;
            (0..n)
                .map(|_| {
                    t += if rng.below(10) == 0 {
                        rng.range(1, 3_600_000)
                    } else {
                        rng.range(1, 5)
                    };
                    t
                })
                .collect()
        }
        Kind::Extremes => {
            let pool = [i64::MIN, i64::MAX, 0, 1, -1, i64::MIN + 1, i64::MAX - 1];
            (0..n)
                .map(|_| pool[rng.below(pool.len() as u64) as usize])
                .collect()
        }
        Kind::Unsorted => (0..n).map(|_| t0 + rng.range(-100_000, 100_000)).collect(),
        _ => (0..n as i64).map(|i| t0 + i * step).collect(),
    };
    let mut walk = 20.0 + rng.f64() * 10.0;
    let mut counter = 0u64;
    let base = 3.14159 * (1 + rng.below(100)) as f64;
    let vals: Vec<f64> = (0..n)
        .map(|_| match kind {
            Kind::Constant => base,
            Kind::RandomWalk => {
                walk += rng.gauss() * 0.3;
                walk
            }
            Kind::Decimals => {
                walk += rng.gauss() * 0.5;
                (walk * 10.0).round() / 10.0
            }
            Kind::Counter => {
                counter += rng.below(50);
                counter as f64
            }
            Kind::RandomBits => f64::from_bits(rng.next()),
            Kind::Special => {
                if rng.below(3) == 0 {
                    SPECIALS[rng.below(SPECIALS.len() as u64) as usize]
                } else {
                    walk + rng.f64()
                }
            }
            _ => 100.0 + (rng.below(5) as f64) * 0.5,
        })
        .collect();
    (ts, vals)
}

fn assert_exact(codec: Codec, ts: &[i64], vals: &[f64], got: &[(i64, f64)], ctx: &str) {
    assert_eq!(got.len(), ts.len(), "{ctx} {codec:?}: length");
    for (i, &(t, v)) in got.iter().enumerate() {
        assert_eq!(t, ts[i], "{ctx} {codec:?}: ts[{i}]");
        assert_eq!(
            v.to_bits(),
            vals[i].to_bits(),
            "{ctx} {codec:?}: value[{i}] {} vs {}",
            v,
            vals[i]
        );
    }
}

// ---------------------------------------------------------------- tests
fn test_bit_io() {
    let mut w = BitWriter::default();
    w.bits(0b101, 3);
    w.bits(u64::MAX, 64);
    w.bits(0, 0);
    w.bits(0x1234, 13);
    w.bit(true);
    let mut r = BitReader::new(&w.buf);
    assert_eq!(r.bits(3), Some(0b101));
    assert_eq!(r.bits(64), Some(u64::MAX));
    assert_eq!(r.bits(13), Some(0x1234 & 0x1fff));
    assert_eq!(r.bit(), Some(true));
    assert_eq!(w.nbits, 81);
    assert!(r.bits(8).is_none() || w.buf.len() * 8 >= r.pos + 8);
    // dod bucket boundaries each cost the documented number of bits
    for (dod, cost) in [
        (0i64, 1),
        (1, 9),
        (-63, 9),
        (64, 9),
        (65, 12),
        (-255, 12),
        (256, 12),
        (257, 16),
        (2048, 16),
        (2049, 68),
        (i64::MIN, 68),
    ] {
        let (mut w, mut s) = (BitWriter::default(), TsState::default());
        s.put(&mut w, 0);
        s.put(&mut w, 0); // delta 0
        let before = w.nbits;
        s.put(&mut w, dod); // delta = dod, previous delta 0
        assert_eq!(w.nbits - before, cost, "dod {dod}");
    }
    println!("bit I/O and dod bucket sizes: ok");
}

fn test_round_trips(rng: &mut Rng) {
    let mut sizes = [0usize; 4];
    let mut samples = 0usize;
    for case in 0..3000 {
        let kind = KINDS[case % KINDS.len()];
        let n = match rng.below(8) {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => rng.below(300) as usize,
        };
        let (ts, vals) = gen_series(kind, n, rng);
        for (ci, &codec) in CODECS.iter().enumerate() {
            let bytes = encode_chunk(codec, &ts, &vals);
            let got = decode_chunk(&bytes)
                .unwrap_or_else(|e| panic!("case {case} {kind:?} {codec:?}: {e:?}"));
            assert_exact(codec, &ts, &vals, &got, &format!("case {case} {kind:?}"));
            sizes[ci] += bytes.len();
            // truncation must be a clean error: drop one byte (>= 1 real bit is lost) and a random prefix
            if n > 0 {
                assert_eq!(
                    decode_chunk(&bytes[..bytes.len() - 1]),
                    Err(DecodeError::Truncated),
                    "{kind:?} {codec:?} trunc"
                );
                let cut = rng.below(bytes.len() as u64) as usize;
                assert!(decode_chunk(&bytes[..cut]).is_err());
            }
        }
        samples += n;
        // integer path: timestamps with order 2, arbitrary ints with order 1, including i64 extremes
        for order in [1, 2] {
            let ints: Vec<i64> = if kind == Kind::Extremes || kind == Kind::Unsorted {
                ts.clone()
            } else {
                ts.iter().map(|&t| t - 1_700_000_000_000).collect()
            };
            let enc = encode_ints(&ints, order);
            assert_eq!(
                decode_ints(&enc).as_deref(),
                Some(&ints[..]),
                "ints {kind:?} order {order}"
            );
        }
    }
    println!(
        "round-trip: 3000 series x 4 codecs bit-exact ({samples} samples/codec); total bytes {sizes:?} (Gorilla/Chimp/Chimp128/Elf)"
    );
}

fn test_policies() {
    let mut c = Chunk::new(Codec::Gorilla);
    c.append(100, 1.0).unwrap();
    c.append(100, 2.0).unwrap(); // duplicate timestamp is allowed
    c.append(160, 3.0).unwrap();
    assert_eq!(
        c.append(150, 4.0),
        Err(AppendError::OutOfOrder {
            last: 160,
            got: 150
        })
    );
    assert_eq!(c.count, 3, "rejected sample must not be written");
    let got = decode_chunk(&c.finish()).unwrap();
    assert_eq!(got, vec![(100, 1.0), (100, 2.0), (160, 3.0)]);
    assert_eq!(decode_chunk(b"nope"), Err(DecodeError::BadHeader));
    let mut hdr = MAGIC.to_vec();
    hdr.extend_from_slice(&[9, 0, 0, 0, 0]);
    assert_eq!(
        decode_chunk(&hdr),
        Err(DecodeError::BadHeader),
        "unknown codec id"
    );
    let mut lying = MAGIC.to_vec();
    lying.extend_from_slice(&[0, 255, 255, 255, 255, 0]);
    assert_eq!(
        decode_chunk(&lying),
        Err(DecodeError::Truncated),
        "huge count with tiny payload"
    );
    // Elf erase is lossless and actually erases on decimals
    let (d, e) = elf_erase(21.3).unwrap();
    assert_eq!(d, 1);
    assert!(
        e.to_bits().trailing_zeros() >= 30,
        "21.3 should lose many mantissa bits, got {}",
        e.to_bits().trailing_zeros()
    );
    assert_eq!(format!("{e:.1}").parse::<f64>().unwrap(), 21.3);
    assert!(
        elf_erase(f64::NAN).is_none() && elf_erase(0.0).is_none() && elf_erase(0.1 + 0.2).is_none()
    );
    println!("policies (reject out-of-order, header/lying-count errors, Elf erase): ok");
}

fn test_simple8b_and_tags(rng: &mut Rng) {
    for _ in 0..500 {
        let n = rng.below(500) as usize;
        let width = [0u32, 1, 3, 8, 20, 40, 60][rng.below(7) as usize];
        let vals: Vec<u64> = (0..n)
            .map(|_| {
                if width == 0 {
                    0
                } else {
                    rng.next() >> (64 - width)
                }
            })
            .collect();
        let words = simple8b_pack(&vals).unwrap();
        assert_eq!(simple8b_unpack(&words, n).unwrap(), vals);
        assert!(
            words.len() <= n.max(1),
            "never worse than one word per value"
        );
    }
    assert!(
        simple8b_pack(&[1 << 60]).is_none(),
        "values >= 2^60 need the raw fallback"
    );
    assert_eq!(
        simple8b_pack(&vec![0u64; 480]).unwrap().len(),
        2,
        "480 zeros = two 240-runs"
    );
    assert!(
        (-3i64..3)
            .chain([i64::MIN, i64::MAX])
            .all(|x| unzigzag(zigzag(x)) == x)
    );
    // tags
    let hosts = ["web-01", "web-02", "db-01", "cache-7", ""];
    let mut raw = 0;
    let mut enc = 0;
    for _ in 0..300 {
        let mut col = Vec::new();
        while col.len() < rng.below(400) as usize {
            let h = hosts[rng.below(hosts.len() as u64) as usize].to_string();
            let run = 1 + rng.below(30) as usize;
            col.extend(std::iter::repeat_n(h, run));
        }
        let b = encode_tags(&col);
        assert_eq!(decode_tags(&b).unwrap(), col);
        raw += col.iter().map(|s| s.len() + 1).sum::<usize>();
        enc += b.len();
        assert!(decode_tags(&b[..b.len().saturating_sub(1)]).is_none() || b.is_empty());
    }
    println!(
        "simple8b + dictionary/RLE tags: ok (tag column bytes {raw} raw -> {enc} encoded, {:.1}x)",
        raw as f64 / enc as f64
    );
}

fn table(rng: &mut Rng) {
    const N: usize = 10_000;
    println!(
        "\nbits per sample (raw = 128), N = {N}; ts = timestamp stream, then value stream per codec"
    );
    println!(
        "{:<11}{:>7} | {:>9}{:>9}{:>9}{:>9} | {:>7}",
        "dataset", "ts", "Gorilla", "Chimp", "Chimp128", "Elf", "best/128"
    );
    let mut result: HashMap<(Kind, Codec), f64> = HashMap::new();
    let mut ts_cost: HashMap<Kind, f64> = HashMap::new();
    for kind in [
        Kind::Regular,
        Kind::Jittery,
        Kind::Bursty,
        Kind::Constant,
        Kind::RandomWalk,
        Kind::Decimals,
        Kind::Counter,
        Kind::RandomBits,
    ] {
        let (ts, vals) = gen_series(kind, N, rng);
        let tsb = ts_bits(&ts) as f64 / N as f64;
        ts_cost.insert(kind, tsb);
        let mut row = String::new();
        let mut best = f64::MAX;
        for codec in CODECS {
            let vb = val_bits(codec, &vals) as f64 / N as f64;
            result.insert((kind, codec), vb);
            best = best.min(vb);
            row += &format!("{vb:>9.2}");
        }
        println!(
            "{:<11}{tsb:>7.2} | {row} | {:>6.1}x",
            format!("{kind:?}"),
            128.0 / (tsb + best)
        );
    }
    // Headline guarantees of the paper's design
    assert!(
        ts_cost[&Kind::Regular] < 1.05,
        "regular timestamps ~1 bit, got {}",
        ts_cost[&Kind::Regular]
    );
    assert!(
        result[&(Kind::Constant, Codec::Gorilla)] < 1.05,
        "constant values ~1 bit with Gorilla"
    );
    assert!(
        result[&(Kind::Constant, Codec::Chimp)] > 1.95
            && result[&(Kind::Constant, Codec::Chimp)] < 2.05,
        "Chimp pays 2 bits for 'same'"
    );
    assert!(
        ts_cost[&Kind::Jittery] > 5.0 && ts_cost[&Kind::Jittery] < 20.0,
        "jitter costs 8-16 bits per timestamp"
    );
    assert!(
        result[&(Kind::Decimals, Codec::Elf)] < result[&(Kind::Decimals, Codec::Gorilla)],
        "Elf erasing wins on 1-decimal sensor data"
    );
    assert!(
        result[&(Kind::RandomBits, Codec::Gorilla)] > 64.0,
        "incompressible bits expand slightly, never shrink"
    );
    assert!(
        result[&(Kind::Counter, Codec::Gorilla)] < 40.0,
        "integer-valued floats have long trailing zero runs"
    );

    // integer packing of the same regular timestamps and of a bursty one, in bytes
    let (ts, _) = gen_series(Kind::Regular, N, rng);
    let rel: Vec<i64> = ts.iter().map(|&t| t - ts[0]).collect();
    let (d1, d2) = (encode_ints(&rel, 1), encode_ints(&rel, 2));
    println!(
        "\nregular timestamps as integers: raw {} B, delta+zigzag+simple8b {} B, delta-of-delta {} B ({:.2} bits/ts); Gorilla ts stream {} B",
        N * 8,
        d1.len(),
        d2.len(),
        d2.len() as f64 * 8.0 / N as f64,
        ts_bits(&ts) / 8
    );
    assert!(d2.len() < d1.len() && d2.len() < 500);
}

fn main() {
    let mut rng = Rng(0x6071_1A5E_ED00_0860);
    test_bit_io();
    test_policies();
    test_simple8b_and_tags(&mut rng);
    test_round_trips(&mut rng);
    table(&mut rng);
    println!("\nall timeseries_gorilla checks passed");
}
