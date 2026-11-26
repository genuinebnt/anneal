//! RECIPE 186 (randomness): a PRNG zoo: LCG, xorshift64*, SplitMix64, PCG32, ChaCha8, and how to test them.
//! LEVEL: intermediate
//! Use in: simulations, tests/fuzzing, sampling, shuffling, load generators (NOT secrets).   Run: cargo run --release --example prng_zoo
//!
//! WHAT: a pseudo-random generator is a deterministic state machine whose output looks random to the tests
//! you care about. Which tests matter is the whole game: an LCG is fine for a toy dice roll and awful when you
//! look at its low bits (bit k of a power-of-two-modulus LCG has period 2^(k+1): bit 0 just alternates).
//! HOW: each generator implements `Rng32`. On top of that we build the parts people get wrong: unbiased bounded
//! integers (Lemire's multiply-shift instead of `% n`, which over-weights small residues), floats in [0,1) from
//! 53 random bits, Gaussians by Box-Muller, and Fisher-Yates shuffling. Then four cheap statistical sanity tests
//! (monobit, byte chi-square, lag-1 serial correlation, runs) with generous thresholds: they cannot certify
//! quality (use PractRand/TestU01 for that) but they catch gross defects, and the LCG fails one.
//! SECURITY: none of these except the ChaCha-based one is designed against an attacker; xorshift/PCG/LCG
//! state is recoverable from a few outputs. Secrets, tokens and keys need an OS CSPRNG (`getrandom`, or
//! `rand::rngs::OsRng` / `StdRng`); even ChaCha8 here is a teaching version (reduced rounds, naive seeding).
//!
//! COMMON USES: reproducible simulation and property tests (seed it, log the seed), Monte Carlo, game logic,
//! randomized algorithms (hashing, skip lists, treaps), load/chaos generators, deterministic fuzzing corpora.
//! In real code use the `rand` crate https://docs.rs/rand (with `rand_pcg`/`rand_chacha` for these algorithms).
//! Read: https://www.pcg-random.org   https://www.rfc-editor.org/rfc/rfc7539   https://arxiv.org/abs/1805.10941 (Lemire)
#![allow(dead_code)]

trait Rng32 {
    fn next_u32(&mut self) -> u32;
    fn next_u64(&mut self) -> u64 {
        (self.next_u32() as u64) << 32 | self.next_u32() as u64
    }
}

// ---------- LCG: x' = a*x + c mod 2^31 (the classic ANSI-C-style `rand`) ----------
struct Lcg(u32);
impl Rng32 for Lcg {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12_345) & 0x7FFF_FFFF;
        self.0 // returning raw state exposes the weak low bits, as old libc rand() did
    }
}

// ---------- xorshift64* ----------
struct XorShift64Star(u64);
impl Rng32 for XorShift64Star {
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) // the multiply scrambles the weak low bits of plain xorshift
    }
}

// ---------- SplitMix64: great for seeding others ----------
struct SplitMix64(u64);
impl Rng32 for SplitMix64 {
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

// ---------- PCG32 (XSH-RR) ----------
struct Pcg32 {
    state: u64,
    inc: u64, // stream selector; must be odd
}
impl Pcg32 {
    const MUL: u64 = 6_364_136_223_846_793_005;
    fn new(seed: u64, stream: u64) -> Self {
        let mut p = Pcg32 {
            state: 0,
            inc: stream << 1 | 1,
        };
        p.next_u32();
        p.state = p.state.wrapping_add(seed);
        p.next_u32();
        p
    }
}
impl Rng32 for Pcg32 {
    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MUL).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32; // "XSH": fold high bits down
        xorshifted.rotate_right((old >> 59) as u32) // "RR": rotate by the top 5 bits
    }
}

// ---------- ChaCha ----------
fn quarter_round(mut a: u32, mut b: u32, mut c: u32, mut d: u32) -> (u32, u32, u32, u32) {
    a = a.wrapping_add(b);
    d = (d ^ a).rotate_left(16);
    c = c.wrapping_add(d);
    b = (b ^ c).rotate_left(12);
    a = a.wrapping_add(b);
    d = (d ^ a).rotate_left(8);
    c = c.wrapping_add(d);
    b = (b ^ c).rotate_left(7);
    (a, b, c, d)
}
fn qr_at(s: &mut [u32; 16], [a, b, c, d]: [usize; 4]) {
    (s[a], s[b], s[c], s[d]) = quarter_round(s[a], s[b], s[c], s[d]);
}
/// ChaCha block function: `rounds` rounds (8/12/20) over a 4x4 state, then add the input (makes it one-way).
fn chacha_block(init: &[u32; 16], rounds: usize) -> [u32; 16] {
    let mut s = *init;
    for _ in 0..rounds / 2 {
        for idx in [[0, 4, 8, 12], [1, 5, 9, 13], [2, 6, 10, 14], [3, 7, 11, 15]] {
            qr_at(&mut s, idx); // columns
        }
        for idx in [[0, 5, 10, 15], [1, 6, 11, 12], [2, 7, 8, 13], [3, 4, 9, 14]] {
            qr_at(&mut s, idx); // diagonals
        }
    }
    for (x, i) in s.iter_mut().zip(init) {
        *x = x.wrapping_add(*i);
    }
    s
}

struct ChaCha8 {
    state: [u32; 16],
    buf: [u32; 16],
    idx: usize,
}
impl ChaCha8 {
    fn from_seed(seed: u64) -> Self {
        let mut sm = SplitMix64(seed); // stretch the 64-bit seed into a 256-bit key
        let mut state = [0u32; 16];
        state[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]); // "expand 32-byte k"
        state[4..12].fill_with(|| sm.next_u32());
        ChaCha8 {
            state,
            buf: [0; 16],
            idx: 16,
        }
    }
}
impl Rng32 for ChaCha8 {
    fn next_u32(&mut self) -> u32 {
        if self.idx == 16 {
            self.buf = chacha_block(&self.state, 8);
            let ctr = (self.state[12] as u64 | (self.state[13] as u64) << 32).wrapping_add(1);
            (self.state[12], self.state[13]) = (ctr as u32, (ctr >> 32) as u32);
            self.idx = 0;
        }
        self.idx += 1;
        self.buf[self.idx - 1]
    }
}

// ---------- derived distributions ----------
/// `% n` on a `bits`-wide source: biased, because 2^bits is not a multiple of n.
fn biased(n: u32, mut src: impl FnMut() -> u32) -> u32 {
    src() % n
}
/// Lemire: map a `bits`-wide uniform x to [0,n) with (x*n) >> bits; reject the few low products that would
/// make some outputs one count more likely. No division on the fast path.
fn lemire(bits: u32, n: u32, mut src: impl FnMut() -> u32) -> u32 {
    let mask = (1u64 << bits) - 1;
    let mut m = src() as u64 * n as u64;
    if (m & mask) < n as u64 {
        let threshold = ((1u64 << bits) - n as u64) % n as u64;
        while (m & mask) < threshold {
            m = src() as u64 * n as u64;
        }
    }
    (m >> bits) as u32
}
fn below(rng: &mut impl Rng32, n: u32) -> u32 {
    lemire(32, n, || rng.next_u32())
}
fn unit_f64(rng: &mut impl Rng32) -> f64 {
    (rng.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64) // 53 bits = the f64 mantissa: exact, never 1.0
}
fn gaussian_pair(rng: &mut impl Rng32) -> (f64, f64) {
    let u1 = 1.0 - unit_f64(rng); // (0,1] so ln() is finite
    let u2 = unit_f64(rng);
    let r = (-2.0 * u1.ln()).sqrt();
    let t = std::f64::consts::TAU * u2;
    (r * t.cos(), r * t.sin())
}
fn shuffle<T>(rng: &mut impl Rng32, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        v.swap(i, below(rng, i as u32 + 1) as usize); // pick from the not-yet-fixed prefix [0, i]
    }
}
fn naive_shuffle<T>(rng: &mut impl Rng32, v: &mut [T]) {
    for i in 0..v.len() {
        v.swap(i, below(rng, v.len() as u32) as usize); // classic bug: n^n outcomes cannot map evenly to n! perms
    }
}

// ---------- statistics ----------
fn chi_square(observed: &[u64], expected: f64) -> f64 {
    observed
        .iter()
        .map(|&o| (o as f64 - expected).powi(2) / expected)
        .sum()
}

struct Report {
    monobit_z: f64,
    chi2: f64,
    serial_r: f64,
    runs_z: f64,
}
impl Report {
    /// df=255 chi-square has mean 255, sd ~22.6: accepting 150..400 is about +-5 sd, so a good generator never trips it,
    /// and "too regular" (chi near 0, as a permutation-like LCG byte stream gives) fails too.
    fn passes(&self) -> bool {
        self.monobit_z.abs() < 5.0
            && self.runs_z.abs() < 5.0
            && self.serial_r.abs() < 0.01
            && (150.0..400.0).contains(&self.chi2)
    }
}

fn analyze(rng: &mut impl Rng32, n: usize) -> Report {
    let outs: Vec<u32> = (0..n).map(|_| rng.next_u32()).collect();
    // Bit tests look at the LOWEST bit of each output, where weak generators hide their structure.
    let bits: Vec<bool> = outs.iter().map(|&x| x & 1 == 1).collect();
    let ones = bits.iter().filter(|&&b| b).count() as f64;
    let nf = n as f64;
    let monobit_z = (ones - nf / 2.0) / (nf / 4.0).sqrt();
    let runs = 1 + bits.windows(2).filter(|w| w[0] != w[1]).count();
    let (n1, n0) = (ones, nf - ones);
    let mean = 1.0 + 2.0 * n1 * n0 / nf;
    let var = 2.0 * n1 * n0 * (2.0 * n1 * n0 - nf) / (nf * nf * (nf - 1.0));
    let runs_z = (runs as f64 - mean) / var.sqrt();
    // Byte tests use the low byte of each output.
    let bytes: Vec<u8> = outs.iter().map(|&x| x as u8).collect();
    let mut hist = [0u64; 256];
    bytes.iter().for_each(|&b| hist[b as usize] += 1);
    let chi2 = chi_square(&hist, nf / 256.0);
    let m = bytes.iter().map(|&b| b as f64).sum::<f64>() / nf;
    let cov: f64 = bytes
        .windows(2)
        .map(|w| (w[0] as f64 - m) * (w[1] as f64 - m))
        .sum();
    let var_b: f64 = bytes.iter().map(|&b| (b as f64 - m).powi(2)).sum();
    Report {
        monobit_z,
        chi2,
        serial_r: cov / var_b,
        runs_z,
    }
}

fn main() {
    // Reference vectors pin the implementations.
    assert_eq!(
        quarter_round(0x1111_1111, 0x0102_0304, 0x9b8d_6f43, 0x0123_4567),
        (0xea2a_92f4, 0xcb1c_f8ce, 0x4581_472e, 0x5881_c4bb)
    ); // RFC 7539 2.1.1
    let mut sm = SplitMix64(0);
    assert_eq!(
        [sm.next_u64(), sm.next_u64()],
        [0xE220_A839_7B1D_CDAF, 0x6E78_9E6A_A1B9_65F4]
    );
    let mut pcg = Pcg32::new(42, 54); // the pcg-random.org demo seed
    assert_eq!(
        [0; 3].map(|_| pcg.next_u32()),
        [0xa15c_02b7, 0x7b47_f409, 0xba1d_3330]
    );
    let key: Vec<u32> = (0..8u32)
        .map(|i| (4 * i) | (4 * i + 1) << 8 | (4 * i + 2) << 16 | (4 * i + 3) << 24)
        .collect();
    let mut st = [0u32; 16];
    st[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]);
    st[4..12].copy_from_slice(&key);
    st[12..].copy_from_slice(&[1, 0x0900_0000, 0x4a00_0000, 0]);
    let blk = chacha_block(&st, 20); // RFC 7539 2.3.2 (the 20-round variant of the same function)
    assert_eq!(
        &blk[..4],
        &[0xe4e7_f110, 0x1559_3bd1, 0x1fdd_0f50, 0xc471_20a3]
    );

    // LCG weakness: the low k bits repeat with period 2^k.
    let mut lcg = Lcg(1);
    let seq: Vec<u32> = (0..64).map(|_| lcg.next_u32()).collect();
    assert!(
        seq.windows(2).all(|w| (w[0] ^ w[1]) & 1 == 1),
        "bit 0 strictly alternates"
    );
    assert!(
        (0..48).all(|i| seq[i] & 0xF == seq[i + 16] & 0xF),
        "low 4 bits repeat every 16 draws"
    );
    println!(
        "LCG low bit over 16 draws: {:?}",
        seq[..16].iter().map(|x| x & 1).collect::<Vec<_>>()
    );

    // Seeding: xorshift is stuck at the all-zero state; SplitMix is the standard way to expand a small seed.
    assert_eq!(XorShift64Star(0).next_u64(), 0);
    let mut seeder = SplitMix64(0);
    let mut xs = XorShift64Star(seeder.next_u64());
    assert_ne!(xs.next_u64(), 0);

    // Statistical sanity: who passes?
    const N: usize = 1_000_000;
    println!(
        "{:<14} {:>9} {:>8} {:>9} {:>8}  verdict",
        "generator", "monobit z", "chi2", "serial r", "runs z"
    );
    let reports: Vec<(&str, Report, bool)> = vec![
        ("LCG (2^31)", analyze(&mut Lcg(12345), N), false),
        (
            "xorshift64*",
            analyze(&mut XorShift64Star(seeder.next_u64()), N),
            true,
        ),
        ("SplitMix64", analyze(&mut SplitMix64(7), N), true),
        ("PCG32", analyze(&mut Pcg32::new(7, 1), N), true),
        ("ChaCha8", analyze(&mut ChaCha8::from_seed(7), N), true),
    ];
    for (name, r, expect_pass) in &reports {
        println!(
            "{name:<14} {:>9.2} {:>8.1} {:>9.4} {:>8.1}  {}",
            r.monobit_z,
            r.chi2,
            r.serial_r,
            r.runs_z,
            if r.passes() { "pass" } else { "FAIL" }
        );
        assert_eq!(r.passes(), *expect_pass, "{name}");
    }

    // Modulo bias: an 8-bit source mod 6 (256 = 42*6 + 4, so residues 0..3 get one extra preimage).
    let mut src = Pcg32::new(99, 3);
    let (mut hb, mut hl) = ([0u64; 6], [0u64; 6]);
    for _ in 0..600_000 {
        hb[biased(6, || src.next_u32() & 0xFF) as usize] += 1;
        hl[lemire(8, 6, || src.next_u32() & 0xFF) as usize] += 1;
    }
    let (cb, cl) = (chi_square(&hb, 100_000.0), chi_square(&hl, 100_000.0));
    println!(
        "\nbounded [0,6) from 8-bit source: `%` chi2={cb:.1}  lemire chi2={cl:.1}  (df=5, 99.99% < 25.7)"
    );
    assert!(cb > 60.0 && cl < 40.0);
    let mut hist = [0u64; 10];
    let mut pcg = Pcg32::new(5, 5);
    (0..200_000).for_each(|_| hist[below(&mut pcg, 10) as usize] += 1);
    assert!(chi_square(&hist, 20_000.0) < 40.0);

    // Floats and Gaussians.
    let mut rng = ChaCha8::from_seed(2024);
    let fs: Vec<f64> = (0..100_000).map(|_| unit_f64(&mut rng)).collect();
    assert!(fs.iter().all(|&f| (0.0..1.0).contains(&f)));
    assert!((fs.iter().sum::<f64>() / 1e5 - 0.5).abs() < 0.01);
    let gs: Vec<f64> = (0..100_000)
        .flat_map(|_| {
            let (a, b) = gaussian_pair(&mut rng);
            [a, b]
        })
        .collect();
    let mean = gs.iter().sum::<f64>() / gs.len() as f64;
    let var = gs.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / gs.len() as f64;
    println!("gaussian: mean={mean:.4} var={var:.4} (want 0, 1)");
    assert!(mean.abs() < 0.02 && (var - 1.0).abs() < 0.03);
    let within_1sd = gs.iter().filter(|g| g.abs() < 1.0).count() as f64 / gs.len() as f64;
    assert!((within_1sd - 0.6827).abs() < 0.01);

    // Shuffle: Fisher-Yates hits all 24 permutations of 4 items evenly; the naive swap does not.
    let tally = |f: fn(&mut Pcg32, &mut [u8])| {
        let mut rng = Pcg32::new(11, 2);
        let mut counts = std::collections::HashMap::new();
        for _ in 0..240_000 {
            let mut v = [0u8, 1, 2, 3];
            f(&mut rng, &mut v);
            *counts.entry(v).or_insert(0u64) += 1;
        }
        assert_eq!(counts.len(), 24);
        chi_square(&counts.values().copied().collect::<Vec<_>>(), 10_000.0)
    };
    let (good, bad) = (tally(shuffle::<u8>), tally(naive_shuffle::<u8>));
    println!("shuffle chi2 (df=23): fisher-yates={good:.1}  naive={bad:.1}");
    assert!(good < 60.0 && bad > 300.0);

    println!("never use these for secrets: use the OS CSPRNG (getrandom / rand::rngs::OsRng).");
    println!("ok");
}
