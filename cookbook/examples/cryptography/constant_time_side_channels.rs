//! RECIPE 224 (crypto): constant-time code and side channels: timing leaks in `==`, a timing attack, ct_select/ct_eq, cache leaks, zeroization.
//! LEVEL: intermediate
//! Use in: MAC/token/password-hash comparison, crypto code review, handling key material in memory.   Run: cargo run --release --example constant_time_side_channels
#![allow(dead_code)]
//!
//! !!! EDUCATIONAL IMPLEMENTATION. NOT constant-time-audited, NOT for production. Compilers and CPUs can reintroduce
//! !!! secret-dependent branches or timing behind your back. Use audited libraries: the `subtle` crate
//! !!! (https://docs.rs/subtle) for constant-time ops, the `zeroize` crate (https://docs.rs/zeroize) for wiping memory, and
//! !!! RustCrypto (https://github.com/RustCrypto) / ring (https://github.com/briansmith/ring) for the primitives. Never roll your own.
//!
//! WHAT / WHY: a side channel leaks secrets through HOW a computation runs (time, cache state, power) rather than WHAT it outputs.
//!  1. `a == b` on byte slices stops at the first difference, so response time reveals the length of the matching prefix.
//!     Checking a MAC or token with `==` lets an attacker guess it one byte at a time: 256*len guesses instead of 256^len.
//!     We (a) measure the naive vs constant-time comparison on this machine (printed; the assertion is generous and
//!     retried, and degrades to a printed note on a noisy machine), and (b) run the attack DETERMINISTICALLY against a simulated
//!     oracle whose "time" is the number of comparison steps (plus injectable jitter), recovering a full secret.
//!  2. Constant-time building blocks: bit masks instead of `if` (`ct_select`, `ct_eq` on u32/u64), OR-accumulating XOR
//!     differences, and `std::hint::black_box` to discourage the optimiser from re-introducing an early exit (it is a hint,
//!     not a guarantee; real code is verified at the assembly level or with tools such as dudect).
//!  3. Branches and table lookups indexed by secrets leak: a branch predictor and a cache are shared hardware state. AES
//!     T-table implementations were broken by cache-timing attacks (Bernstein 2005; Osvik-Shamir-Tromer 2006); modern AES uses
//!     AES-NI or bitsliced code. We model a 64-byte-line cache and show the touched line reveals the high bits of key^plaintext.
//!  4. Zeroization: secrets should not linger in memory (swap, core dumps, later bugs). A plain `buf.fill(0)` / `x = 0` right
//!     before a value dies is a "dead store" the optimiser may delete. A volatile write plus a compiler fence cannot be removed.
//!     This protects against the optimiser only, not against copies the compiler/stack/registers already made.
//!
//! COMMON USES: constant-time HMAC/tag comparison (TLS, JWT and webhook-signature verification), password-hash verification,
//! constant-time scalar multiplication in elliptic-curve libraries, bitsliced or hardware AES instead of T-tables, and
//! zeroizing keys on drop in key-management code.
//! Read: https://docs.rs/subtle
//! Read: https://docs.rs/zeroize
//! Read: https://cr.yp.to/antiforgery/cachetiming-20050414.pdf (Bernstein, cache-timing attacks on AES)
use std::hint::black_box;
use std::sync::atomic::{Ordering, compiler_fence};
use std::time::Instant;

// ---------------------------------------------------------------- comparisons

/// Early-exit comparison, like `==` on slices. Returns (equal, steps) where `steps` is the number of bytes examined.
fn naive_eq_counted(a: &[u8], b: &[u8]) -> (bool, u64) {
    if a.len() != b.len() {
        return (false, 0);
    }
    let mut steps = 0;
    for i in 0..a.len() {
        steps += 1;
        if a[i] != b[i] {
            return (false, steps); // <-- the leak: work done depends on the first differing position
        }
    }
    (true, steps)
}

fn naive_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

/// Always examines every byte. (Length is public information, so branching on it is fine.)
fn ct_eq_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    black_box(diff) == 0
}

/// Counted variant for the simulation: steps are always `len`.
fn ct_eq_counted(a: &[u8], b: &[u8]) -> (bool, u64) {
    (ct_eq_bytes(a, b), a.len() as u64)
}

// ---------------------------------------------------------------- masks

/// 0xFFFF_FFFF if `cond` (which must be 0 or 1), else 0.
fn mask32(cond: u32) -> u32 {
    0u32.wrapping_sub(cond)
}
fn ct_select_u32(cond: u32, a: u32, b: u32) -> u32 {
    let m = mask32(cond);
    (a & m) | (b & !m)
}
fn ct_select_u64(cond: u64, a: u64, b: u64) -> u64 {
    let m = 0u64.wrapping_sub(cond);
    (a & m) | (b & !m)
}
/// Returns 1 if a == b else 0, without branching: x|-x has its top bit set iff x != 0.
fn ct_eq_u32(a: u32, b: u32) -> u32 {
    let x = a ^ b;
    ((x | x.wrapping_neg()) >> 31) ^ 1
}
fn ct_eq_u64(a: u64, b: u64) -> u64 {
    let x = a ^ b;
    ((x | x.wrapping_neg()) >> 63) ^ 1
}
/// 1 if a < b (unsigned), else 0. Borrow-bit trick, no branch.
fn ct_lt_u32(a: u32, b: u32) -> u32 {
    (((a ^ ((a ^ b) | (a.wrapping_sub(b) ^ b))) as u32) >> 31) & 1
}

// ---------------------------------------------------------------- timing measurement

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Median over `samples` of the mean ns/call over `iters` calls.
fn time_ns(samples: usize, iters: usize, mut f: impl FnMut() -> bool) -> f64 {
    let mut v: Vec<f64> = (0..samples)
        .map(|_| {
            let t = Instant::now();
            let mut acc = 0usize;
            for _ in 0..iters {
                acc += black_box(f()) as usize;
            }
            black_box(acc);
            t.elapsed().as_nanos() as f64 / iters as f64
        })
        .collect();
    median(&mut v)
}

fn measure_table() -> (f64, f64, bool) {
    const LEN: usize = 4096;
    let a = vec![0xAAu8; LEN];
    let positions = [0usize, 64, 512, 1024, 2048, LEN - 1];
    println!(
        "  mismatch pos |  naive ns/call | ct ns/call   (len {LEN}, median of 21 samples x 200 calls)"
    );
    let (mut first, mut last) = (0.0, 0.0);
    let mut ct_times = vec![];
    for (i, &pos) in positions.iter().enumerate() {
        let mut b = a.clone();
        b[pos] ^= 1;
        let tn = time_ns(21, 200, || naive_eq(black_box(&a), black_box(&b)));
        let tc = time_ns(21, 200, || ct_eq_bytes(black_box(&a), black_box(&b)));
        println!("  {pos:>12} | {tn:>14.1} | {tc:>10.1}");
        ct_times.push(tc);
        if i == 0 {
            first = tn;
        }
        if i == positions.len() - 1 {
            last = tn;
        }
    }
    let ct_min = ct_times.iter().cloned().fold(f64::MAX, f64::min);
    let ct_max = ct_times.iter().cloned().fold(0.0, f64::max);
    println!(
        "  constant-time spread max/min = {:.2} (should be ~1)",
        ct_max / ct_min
    );
    (first, last, last > 3.0 * first)
}

// ---------------------------------------------------------------- simulated timing attack

/// Deterministic cost model: simulated time = steps * per_step + jitter in [0, jitter). Injectable so tests control the noise.
struct CostModel {
    per_step: u64,
    jitter: u64,
    state: u64,
}
impl CostModel {
    fn time(&mut self, steps: u64) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = if self.jitter == 0 {
            0
        } else {
            (self.state >> 33) % self.jitter
        };
        steps * self.per_step + j
    }
}

/// The victim: compares a guess to a secret and reports (accepted, observed simulated time).
struct Oracle<'a> {
    secret: &'a [u8],
    cmp: fn(&[u8], &[u8]) -> (bool, u64),
    cost: CostModel,
    queries: u64,
}
impl Oracle<'_> {
    fn query(&mut self, guess: &[u8]) -> (bool, u64) {
        self.queries += 1;
        let (ok, steps) = (self.cmp)(self.secret, guess);
        (ok, self.cost.time(steps))
    }
}

/// Recover the secret byte by byte: for each position pick the guess byte with the largest observed time.
fn timing_attack(o: &mut Oracle, len: usize) -> Option<Vec<u8>> {
    let mut known: Vec<u8> = Vec::new();
    for pos in 0..len {
        let mut best = (0u8, 0u64);
        for g in 0..=255u8 {
            let mut guess = known.clone();
            guess.push(g);
            guess.resize(len, 0); // padding bytes are arbitrary
            let (ok, t) = o.query(&guess);
            if ok {
                return Some(guess);
            }
            if t > best.1 {
                best = (g, t);
            }
        }
        known.push(best.0);
        let _ = pos;
    }
    let (ok, _) = o.query(&known);
    ok.then_some(known)
}

fn test_attack() {
    let secret = *b"s3cr3t-T0k3n!xyz";
    let mut naive = Oracle {
        secret: &secret,
        cmp: naive_eq_counted,
        cost: CostModel {
            per_step: 10,
            jitter: 8,
            state: 1,
        },
        queries: 0,
    };
    let got = timing_attack(&mut naive, secret.len())
        .expect("attack must succeed against early-exit compare");
    assert_eq!(got, secret);
    let max_q = 256 * secret.len() as u64 + 1;
    assert!(naive.queries <= max_q);
    println!(
        "attack on naive compare: recovered {:?} in {} queries (brute force would need 256^{} ~ 3e38)",
        String::from_utf8_lossy(&got),
        naive.queries,
        secret.len()
    );

    // Against the constant-time compare every guess takes `len` steps: times differ only by jitter -> no signal.
    let mut ct = Oracle {
        secret: &secret,
        cmp: ct_eq_counted,
        cost: CostModel {
            per_step: 10,
            jitter: 0,
            state: 1,
        },
        queries: 0,
    };
    let times: Vec<u64> = (0..=255u8).map(|g| ct.query(&[g; 16]).1).collect();
    assert!(
        times.iter().all(|&t| t == times[0]),
        "ct oracle: identical time for every guess"
    );
    println!(
        "constant-time oracle: all 256 guesses cost {} simulated units: nothing to distinguish",
        times[0]
    );
}

// ---------------------------------------------------------------- branches and tables on secrets

/// Square-and-multiply with a secret-dependent branch: number of multiplies = Hamming weight of the exponent.
fn pow_leaky(mut b: u64, mut e: u64, m: u64, mults: &mut u32) -> u64 {
    let mut r = 1;
    while e > 0 {
        if e & 1 == 1 {
            r = (r as u128 * b as u128 % m as u128) as u64;
            *mults += 1;
        }
        b = (b as u128 * b as u128 % m as u128) as u64;
        e >>= 1;
    }
    r
}
/// Fixed 64 iterations; the multiply always happens and the result is chosen with a mask.
fn pow_ct(mut b: u64, e: u64, m: u64, mults: &mut u32) -> u64 {
    let mut r = 1;
    for i in 0..64 {
        let t = (r as u128 * b as u128 % m as u128) as u64;
        *mults += 1;
        r = ct_select_u64((e >> i) & 1, t, r);
        b = (b as u128 * b as u128 % m as u128) as u64;
    }
    r
}

/// Model of a 256-entry table of 4-byte words in 64-byte cache lines: 16 entries per line, 16 lines.
const ENTRIES_PER_LINE: usize = 16;

fn table_lookup_leaky(table: &[u32; 256], idx: u8, touched: &mut Vec<usize>) -> u32 {
    touched.push(idx as usize / ENTRIES_PER_LINE); // the attacker observes which line was loaded (Prime+Probe / Flush+Reload)
    table[idx as usize]
}
fn table_lookup_ct(table: &[u32; 256], idx: u8, touched: &mut Vec<usize>) -> u32 {
    let mut out = 0;
    for (i, &v) in table.iter().enumerate() {
        touched.push(i / ENTRIES_PER_LINE); // every line is touched, in the same order, every time
        out = ct_select_u32(ct_eq_u32(i as u32, idx as u32), v, out);
    }
    out
}

fn test_leaks() {
    let (mut m1, mut m2) = (0, 0);
    let (e_sparse, e_dense) = (0x8000_0000_0000_0001u64, u64::MAX);
    let (mut a, mut b) = (0, 0);
    let p = 0xffff_ffff_ffff_ffc5u64; // largest 64-bit prime
    assert_eq!(
        pow_leaky(7, e_sparse, p, &mut a),
        pow_ct(7, e_sparse, p, &mut m1)
    );
    assert_eq!(
        pow_leaky(7, e_dense, p, &mut b),
        pow_ct(7, e_dense, p, &mut m2)
    );
    assert_eq!((a, b), (2, 64)); // leak: multiply count equals Hamming weight of the secret exponent
    assert_eq!(m1, 64);
    assert_eq!(m2, 64);
    println!(
        "branchy modexp: {a} multiplies for a sparse exponent vs {b} for a dense one (leaks Hamming weight); ct version: {m1} and {m2}"
    );

    // Cache model: T-table style lookup T[p ^ k] with known plaintext byte p. The attacker sees the line number.
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        *t = (i as u32).wrapping_mul(0x9e37_79b1);
    }
    let key = 0xA7u8;
    let mut candidates: Vec<u8> = (0..=255).collect();
    for p in [0x13u8, 0x5c, 0xe1] {
        let mut touched = vec![];
        table_lookup_leaky(&table, p ^ key, &mut touched);
        let line = touched[0];
        candidates.retain(|&k| (p ^ k) as usize / ENTRIES_PER_LINE == line);
    }
    assert!(candidates.contains(&key));
    assert!(
        candidates.iter().all(|k| k >> 4 == key >> 4),
        "high nibble of the key is recovered from line numbers alone"
    );
    println!(
        "cache model: line numbers reveal key high nibble {:#x}; {} candidate key bytes remain (more lookups narrow further)",
        key >> 4,
        candidates.len()
    );
    let (mut t1, mut t2) = (vec![], vec![]);
    let v1 = table_lookup_ct(&table, 0x12, &mut t1);
    let v2 = table_lookup_ct(&table, 0xF0, &mut t2);
    assert_eq!((v1, v2), (table[0x12], table[0xF0]));
    assert_eq!(
        t1, t2,
        "ct lookup: identical access pattern for every secret index"
    );
    println!(
        "scan-all lookup: identical memory access pattern for different secrets (cost: 256x more work)"
    );
}

fn test_masks() {
    for (a, b) in [
        (0u32, 0u32),
        (1, 0),
        (0, 1),
        (u32::MAX, u32::MAX),
        (0x8000_0000, 0),
        (5, 9),
    ] {
        assert_eq!(ct_eq_u32(a, b), (a == b) as u32);
        assert_eq!(ct_lt_u32(a, b), (a < b) as u32);
        assert_eq!(ct_select_u32(1, a, b), a);
        assert_eq!(ct_select_u32(0, a, b), b);
        assert_eq!(ct_eq_u64(a as u64, b as u64), (a == b) as u64);
        assert_eq!(ct_select_u64(1, a as u64, b as u64), a as u64);
        assert_eq!(ct_select_u64(0, a as u64, b as u64), b as u64);
    }
    let mut x = 88172645463325252u64;
    for _ in 0..10_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let (a, b) = (x as u32, (x >> 32) as u32);
        assert_eq!(ct_lt_u32(a, b), (a < b) as u32);
        assert_eq!(ct_eq_u32(a, a), 1);
        assert_eq!(ct_eq_u64(x, x.wrapping_add(1)), 0);
    }
    assert!(
        ct_eq_bytes(b"abc", b"abc") && !ct_eq_bytes(b"abc", b"abd") && !ct_eq_bytes(b"abc", b"ab")
    );
    println!("masks: ct_select / ct_eq / ct_lt agree with the branching versions");
}

// ---------------------------------------------------------------- zeroization

/// Overwrite with volatile writes (cannot be elided as a dead store) and fence so later code is not reordered before them.
fn zeroize_bytes(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        // SAFETY: `b` is a valid, aligned, exclusive reference derived from a live slice; writing a u8 through it is sound.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

/// A key that wipes itself when dropped. (The `zeroize` crate does this and also handles Vec capacity, arrays, and derives.)
struct SecretKey([u8; 32]);
impl Drop for SecretKey {
    fn drop(&mut self) {
        zeroize_bytes(&mut self.0);
    }
}

fn test_zeroize() {
    use std::mem::ManuallyDrop;
    let mut k = ManuallyDrop::new(SecretKey([0x42; 32]));
    assert!(k.0.iter().all(|&b| b == 0x42));
    // SAFETY: `k` is dropped exactly once here and never used as a SecretKey afterwards; we only inspect the (plain u8) storage.
    unsafe { ManuallyDrop::drop(&mut k) };
    assert!(
        k.0.iter().all(|&b| b == 0),
        "Drop must have wiped the key bytes"
    );
    let mut v = vec![7u8; 16];
    zeroize_bytes(&mut v);
    assert!(v.iter().all(|&b| b == 0));
    println!("zeroize: key bytes are zero after Drop (volatile write + compiler fence)");
    println!(
        "  why not `= 0`? if the buffer is never read again, the optimiser may remove the store as dead; volatile forbids that"
    );
}

fn main() {
    println!(
        "EDUCATIONAL ONLY: not audited, not for production. Use `subtle`, `zeroize`, RustCrypto, ring.\n"
    );
    test_masks();
    println!("\n-- measuring (machine-dependent, printed only) --");
    // Retry a few times; the leak is ~1000x here so a pass is overwhelmingly likely. Never panic on a noisy machine.
    let mut ok = false;
    for attempt in 1..=3 {
        let (first, last, passed) = measure_table();
        println!(
            "  naive: pos 0 = {first:.1} ns, last = {last:.1} ns (x{:.1})  attempt {attempt}",
            last / first.max(0.001)
        );
        if passed {
            ok = true;
            break;
        }
    }
    if ok {
        println!("  OK: naive time grows with matching prefix length");
    } else {
        println!(
            "  NOTE: machine too noisy / optimiser changed the loop; timing check skipped (deterministic simulation below is authoritative)"
        );
    }
    println!("\n-- deterministic attack simulation --");
    test_attack();
    println!("\n-- branches and tables on secrets --");
    test_leaks();
    println!("\n-- zeroization --");
    test_zeroize();
    println!("\nall assertions passed");
}
