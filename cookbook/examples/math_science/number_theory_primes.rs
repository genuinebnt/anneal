//! RECIPE 275 (numerics): number-theory toolkit: sieves (plain/odd/wheel/segmented/linear), phi & mu, Pollard-Brent, CRT, BSGS, Tonelli-Shanks, Fibonacci doubling, Josephus, Pell, Collatz, Lucas.
//! LEVEL: advanced
//! Use in: competitive/contest number theory, crypto groundwork, any code that needs primes, factorisation or modular roots.   Run: cargo run --release --example number_theory_primes
#![allow(dead_code)]
//!
//! WHAT: fifteen classical number-theory algorithms in std-only Rust, EACH checked against a naive oracle in main().
//!  1. Sieve of Eratosthenes: plain `Vec<bool>`, ODD-ONLY (half the memory), a 6k+-1 WHEEL (a third), and a SEGMENTED sieve
//!     that lists primes in [lo, lo+len) for lo ~ 10^12 using only the base primes up to sqrt(hi) (O(sqrt(hi)+len) memory).
//!     Timings are printed; counts are checked against pi(10^7) = 664579.
//!  2. LINEAR sieve (Euler): every composite is crossed off exactly once, by its smallest prime factor, so we get the
//!     smallest-prime-factor table, Euler's phi and the Moebius mu in O(n). Checked against the definitions for n <= 10^4
//!     (phi = count of k <= n coprime to n; mu from trial-division factorisation).
//!  3. Miller-Rabin, deterministic for every u64 (the first 12 primes as witnesses) + POLLARD'S RHO, Brent's variant
//!     (batched gcds, cycle detection by powers of two). 200 random 60-bit semiprimes are split; product and primality of
//!     the factors are asserted. Expected work is O(p^(1/2)) for the smallest prime factor p.
//!  4. CHINESE REMAINDER THEOREM for NON-coprime moduli: combine x = a1 (m1), x = a2 (m2) iff a1 = a2 (mod gcd); the answer
//!     is modulo lcm. Oracle: scan 0..lcm.
//!  5. BABY-STEP GIANT-STEP discrete log g^x = h (mod p) in O(sqrt p) time and memory. Oracle: linear scan.
//!  6. TONELLI-SHANKS: sqrt(a) mod an odd prime p. Oracle: all squares for every prime < 400.
//!  7. FIBONACCI by FAST DOUBLING: F(2k) = F(k)(2F(k+1) - F(k)), F(2k+1) = F(k)^2 + F(k+1)^2, O(log n). Oracles: 2x2 matrix
//!     exponentiation and plain iteration.
//!  8. JOSEPHUS: closed form J(n,2) = 2(n - 2^floor(log2 n)) + 1, and the O(n) recurrence for any k; oracle: simulation.
//!  9. PELL equation x^2 - D y^2 = 1 from the continued fraction of sqrt(D) (convergents); oracle: scan small y.
//! 10. Perfect numbers (Euclid-Euler: 2^(p-1)(2^p - 1) with Mersenne prime via Lucas-Lehmer) vs a sigma sieve; Collatz
//!     stopping times with a CACHE (memoisation of every value below the limit).
//! 11. LUCAS' THEOREM: C(n, k) mod p = prod C(n_i, k_i) over base-p digits. Oracle: Pascal's triangle mod p.
//!
//! WHY (Skiena, the "number theory" corner of the catalog): most contest/crypto problems reduce to a handful of these
//! primitives. The recurring trick is a u128 widening multiply, which makes every 64-bit modulus safe.
//!
//! COMMON USES: primes and factorisation in RSA/DH key generation and in attacks on weak keys; CRT inside RSA and in
//! residue-number-system arithmetic; discrete logs in the analysis of Diffie-Hellman groups; modular square roots in
//! elliptic-curve point decompression (Tonelli-Shanks / Cipolla); fast Fibonacci/linear-recurrence evaluation (matrix
//! powers) in combinatorics; Lucas' theorem for binomials modulo small primes in counting problems; segmented sieves for
//! prime tables far from zero.
//! Read: The Art of Computer Programming Vol 2 (Seminumerical Algorithms), sections 4.5 (arithmetic/factoring) and 4.5.4
//! Read: The Algorithm Design Manual (Skiena), the catalog entries "Factoring and Primality Testing" and "Modular Arithmetic" (chapter 16)
//! Read: https://en.wikipedia.org/wiki/Sieve_of_Eratosthenes
//! Read: https://en.wikipedia.org/wiki/Pollard%27s_rho_algorithm
//! Read: https://en.wikipedia.org/wiki/Miller%E2%80%93Rabin_primality_test
//! Read: https://en.wikipedia.org/wiki/Baby-step_giant-step
//! Read: https://en.wikipedia.org/wiki/Tonelli%E2%80%93Shanks_algorithm
//! Read: https://en.wikipedia.org/wiki/Josephus_problem
//! Read: https://en.wikipedia.org/wiki/Lucas%27s_theorem
use std::collections::HashMap;
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ---------------------------------------------------------------- modular basics
fn mulmod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}
fn powmod(mut b: u64, mut e: u64, m: u64) -> u64 {
    let mut r = 1 % m;
    b %= m;
    while e > 0 {
        if e & 1 == 1 {
            r = mulmod(r, b, m);
        }
        b = mulmod(b, b, m);
        e >>= 1;
    }
    r
}
fn addmod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 + b as u128) % m as u128) as u64
}
fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}
/// Extended Euclid on i128: returns (g, x, y) with a*x + b*y = g.
fn ext_gcd(a: i128, b: i128) -> (i128, i128, i128) {
    if b == 0 {
        (a, 1, 0)
    } else {
        let (g, x, y) = ext_gcd(b, a % b);
        (g, y, x - (a / b) * y)
    }
}
fn modinv(a: u64, m: u64) -> Option<u64> {
    let (g, x, _) = ext_gcd(a as i128, m as i128);
    if g != 1 {
        None
    } else {
        Some(x.rem_euclid(m as i128) as u64)
    }
}

// ---------------------------------------------------------------- 1. sieves
fn sieve_plain(n: usize) -> Vec<bool> {
    let mut is = vec![true; n + 1];
    is[0] = false;
    if n >= 1 {
        is[1] = false;
    }
    let mut i = 2;
    while i * i <= n {
        if is[i] {
            let mut j = i * i;
            while j <= n {
                is[j] = false;
                j += i;
            }
        }
        i += 1;
    }
    is
}

/// Odd-only: slot i represents the number 2i+1. Returns the prime count up to n.
fn sieve_odd_count(n: usize) -> usize {
    if n < 2 {
        return 0;
    }
    let size = (n - 1) / 2 + 1; // numbers 1,3,5,..,<=n
    let mut comp = vec![false; size];
    let mut i = 1;
    while (2 * i + 1) * (2 * i + 1) <= n {
        if !comp[i] {
            let p = 2 * i + 1;
            let mut j = (p * p - 1) / 2;
            while j < size {
                comp[j] = true;
                j += p;
            }
        }
        i += 1;
    }
    1 + (1..size).filter(|&i| !comp[i]).count() // +1 for the prime 2
}

/// Wheel mod 6: only 6k-1 and 6k+1 are stored (idx 2(k-1) and 2(k-1)+1). Returns the prime count up to n.
fn sieve_wheel6_count(n: usize) -> usize {
    let base = [2, 3].iter().filter(|&&p| p <= n).count();
    if n < 5 {
        return base;
    }
    let idx = |x: usize| {
        if x % 6 == 5 {
            2 * (x / 6)
        } else {
            2 * (x / 6) - 1
        }
    };
    let num = |i: usize| {
        if i % 2 == 0 {
            6 * (i / 2 + 1) - 1
        } else {
            6 * (i / 2 + 1) + 1
        }
    };
    let mut size = idx(if n % 6 == 0 { n - 1 } else { n }) + 1;
    // n % 6 in {2,3,4} : largest candidate <= n is 6k-1 or 6k+1 just below; fix up by trimming
    while size > 0 && num(size - 1) > n {
        size -= 1;
    }
    let mut comp = vec![false; size];
    for i in 0..size {
        let p = num(i);
        if p * p > n {
            break;
        }
        if comp[i] {
            continue;
        }
        // multiples p*q for candidates q >= p (q coprime to 6 keeps the product on the wheel)
        let mut j = i;
        while j < size {
            let m = p * num(j);
            if m > n {
                break;
            }
            comp[idx(m)] = true;
            j += 1;
        }
    }
    base + comp.iter().filter(|&&c| !c).count()
}

/// Segmented sieve: all primes in [lo, hi). Memory O(sqrt(hi) + (hi - lo)).
fn segmented_primes(lo: u64, hi: u64) -> Vec<u64> {
    let lim = (hi as f64).sqrt() as usize + 2;
    let small = sieve_plain(lim);
    let mut seg = vec![true; (hi - lo) as usize];
    for p in 2..=lim {
        if !small[p] {
            continue;
        }
        let p64 = p as u64;
        if p64 * p64 >= hi {
            break;
        }
        let mut start = ((lo + p64 - 1) / p64) * p64;
        if start < p64 * p64 {
            start = p64 * p64;
        }
        let mut j = start;
        while j < hi {
            seg[(j - lo) as usize] = false;
            j += p64;
        }
    }
    (lo..hi)
        .filter(|&x| x >= 2 && seg[(x - lo) as usize])
        .collect()
}

// ---------------------------------------------------------------- 2. linear sieve
struct Linear {
    spf: Vec<u32>,
    phi: Vec<u32>,
    mu: Vec<i8>,
    primes: Vec<u32>,
}
fn linear_sieve(n: usize) -> Linear {
    let mut spf = vec![0u32; n + 1];
    let mut phi = vec![0u32; n + 1];
    let mut mu = vec![0i8; n + 1];
    let mut primes: Vec<u32> = vec![];
    phi[1] = 1;
    mu[1] = 1;
    for i in 2..=n {
        if spf[i] == 0 {
            spf[i] = i as u32;
            phi[i] = i as u32 - 1;
            mu[i] = -1;
            primes.push(i as u32);
        }
        for &p in &primes {
            let ip = i * p as usize;
            if p > spf[i] || ip > n {
                break;
            }
            spf[ip] = p; // set exactly once: by the smallest prime factor
            if i % p as usize == 0 {
                phi[ip] = phi[i] * p;
                mu[ip] = 0;
            } else {
                phi[ip] = phi[i] * (p - 1);
                mu[ip] = -mu[i];
            }
        }
    }
    Linear {
        spf,
        phi,
        mu,
        primes,
    }
}
fn trial_factor(mut n: u64) -> Vec<(u64, u32)> {
    let mut f = vec![];
    let mut p = 2;
    while p * p <= n {
        if n % p == 0 {
            let mut e = 0;
            while n % p == 0 {
                n /= p;
                e += 1;
            }
            f.push((p, e));
        }
        p += 1;
    }
    if n > 1 {
        f.push((n, 1));
    }
    f
}

// ---------------------------------------------------------------- 3. Miller-Rabin + Pollard-Brent
fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    const W: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    for &p in &W {
        if n % p == 0 {
            return n == p;
        }
    }
    let (mut d, mut s) = (n - 1, 0);
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    'w: for &a in &W {
        let mut x = powmod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mulmod(x, x, n);
            if x == n - 1 {
                continue 'w;
            }
        }
        return false;
    }
    true
}

/// Brent's cycle-finding rho with batched gcd. Returns a non-trivial factor of composite odd n, or None for this c.
fn brent_rho(n: u64, c: u64) -> Option<u64> {
    let f = |x: u64| addmod(mulmod(x, x, n), c, n);
    let (m, mut g, mut r, mut q) = (128u64, 1u64, 1u64, 1u64);
    let (mut x, mut y, mut ys) = (2u64, 2u64, 2u64);
    while g == 1 {
        x = y;
        for _ in 0..r {
            y = f(y);
        }
        let mut k = 0;
        while k < r && g == 1 {
            ys = y;
            for _ in 0..m.min(r - k) {
                y = f(y);
                q = mulmod(q, x.abs_diff(y), n);
            }
            g = gcd(q, n);
            k += m;
        }
        r *= 2;
    }
    if g == n {
        // the batch overshot: redo it one step at a time
        loop {
            ys = f(ys);
            g = gcd(x.abs_diff(ys), n);
            if g > 1 {
                break;
            }
        }
    }
    if g == n { None } else { Some(g) }
}

fn factor_into(n: u64, out: &mut Vec<u64>) {
    if n == 1 {
        return;
    }
    if is_prime(n) {
        out.push(n);
        return;
    }
    for p in [2u64, 3, 5, 7, 11, 13] {
        if n % p == 0 {
            out.push(p);
            factor_into(n / p, out);
            return;
        }
    }
    let mut c = 1;
    loop {
        if let Some(d) = brent_rho(n, c) {
            factor_into(d, out);
            factor_into(n / d, out);
            return;
        }
        c += 1;
    }
}
fn factorize(n: u64) -> Vec<u64> {
    let mut v = vec![];
    factor_into(n, &mut v);
    v.sort();
    v
}
fn random_prime(rng: &mut Rng, bits: u32) -> u64 {
    loop {
        let c = (rng.next() >> (64 - bits)) | (1 << (bits - 1)) | 1;
        if is_prime(c) {
            return c;
        }
    }
}

// ---------------------------------------------------------------- 4. CRT, non-coprime
/// Combine x = a1 (mod m1), x = a2 (mod m2). Result (a, lcm) or None if inconsistent.
fn crt2(a1: u64, m1: u64, a2: u64, m2: u64) -> Option<(u64, u64)> {
    let g = gcd(m1, m2);
    let diff = a2 as i128 - a1 as i128;
    if diff % g as i128 != 0 {
        return None;
    }
    let l = (m1 / g) as i128 * m2 as i128;
    let (_, p, _) = ext_gcd(m1 as i128, m2 as i128); // m1 p = g (mod m2)
    let m2g = (m2 / g) as i128;
    let t = ((diff / g as i128) % m2g * (p % m2g)).rem_euclid(m2g);
    let x = (a1 as i128 + m1 as i128 * t).rem_euclid(l);
    Some((x as u64, l as u64))
}
fn crt(eqs: &[(u64, u64)]) -> Option<(u64, u64)> {
    let mut cur = (0u64, 1u64);
    for &(a, m) in eqs {
        cur = crt2(cur.0, cur.1, a % m, m)?;
    }
    Some(cur)
}

// ---------------------------------------------------------------- 5. BSGS
/// Smallest x >= 0 with g^x = h (mod p), p prime, or None.
fn bsgs(g: u64, h: u64, p: u64) -> Option<u64> {
    let m = (p as f64).sqrt().ceil() as u64 + 1;
    let mut table: HashMap<u64, u64> = HashMap::with_capacity(m as usize);
    let mut cur = 1u64;
    for j in 0..m {
        table.entry(cur).or_insert(j);
        cur = mulmod(cur, g, p);
    }
    let factor = powmod(modinv(powmod(g, m, p), p)?, 1, p); // g^-m
    let mut gamma = h % p;
    for i in 0..m {
        if let Some(&j) = table.get(&gamma) {
            return Some(i * m + j);
        }
        gamma = mulmod(gamma, factor, p);
    }
    None
}

// ---------------------------------------------------------------- 6. Tonelli-Shanks
fn tonelli_shanks(a: u64, p: u64) -> Option<u64> {
    let a = a % p;
    if a == 0 {
        return Some(0);
    }
    if p == 2 {
        return Some(a);
    }
    if powmod(a, (p - 1) / 2, p) != 1 {
        return None; // non-residue (Euler's criterion)
    }
    let (mut q, mut s) = (p - 1, 0u32);
    while q % 2 == 0 {
        q /= 2;
        s += 1;
    }
    let mut z = 2;
    while powmod(z, (p - 1) / 2, p) != p - 1 {
        z += 1;
    }
    let (mut m, mut c) = (s, powmod(z, q, p));
    let mut t = powmod(a, q, p);
    let mut r = powmod(a, (q + 1) / 2, p);
    while t != 1 {
        let (mut i, mut t2) = (0, t);
        while t2 != 1 {
            t2 = mulmod(t2, t2, p);
            i += 1;
        }
        let b = powmod(c, 1 << (m - i - 1), p);
        m = i;
        c = mulmod(b, b, p);
        t = mulmod(t, c, p);
        r = mulmod(r, b, p);
    }
    Some(r)
}

// ---------------------------------------------------------------- 7. Fibonacci
/// (F(n), F(n+1)) mod m by fast doubling.
fn fib_doubling(n: u64, m: u64) -> (u64, u64) {
    let (mut a, mut b) = (0u64, 1 % m); // F(0), F(1)
    for bit in (0..64).rev() {
        // doubling step: (F(k),F(k+1)) -> (F(2k),F(2k+1))
        let t = addmod(addmod(b, b, m), m - a % m, m);
        let c = mulmod(a, t, m);
        let d = addmod(mulmod(a, a, m), mulmod(b, b, m), m);
        if (n >> bit) & 1 == 1 {
            a = d;
            b = addmod(c, d, m);
        } else {
            a = c;
            b = d;
        }
    }
    (a, b)
}
fn fib_matrix(n: u64, m: u64) -> u64 {
    type M = [[u64; 2]; 2];
    let mul = |x: &M, y: &M| -> M {
        let mut r = [[0u64; 2]; 2];
        for i in 0..2 {
            for j in 0..2 {
                r[i][j] = addmod(mulmod(x[i][0], y[0][j], m), mulmod(x[i][1], y[1][j], m), m);
            }
        }
        r
    };
    let (mut res, mut base, mut e): (M, M, u64) =
        ([[1 % m, 0], [0, 1 % m]], [[1 % m, 1 % m], [1 % m, 0]], n);
    while e > 0 {
        if e & 1 == 1 {
            res = mul(&res, &base);
        }
        base = mul(&base, &base);
        e >>= 1;
    }
    res[0][1]
}
fn fib_iter(n: u64, m: u64) -> u64 {
    let (mut a, mut b) = (0u64, 1 % m);
    for _ in 0..n {
        (a, b) = (b, addmod(a, b, m));
    }
    a
}

// ---------------------------------------------------------------- 8. Josephus
/// 0-indexed survivor, O(n) recurrence J(1)=0, J(i) = (J(i-1)+k) mod i.
fn josephus_rec(n: u64, k: u64) -> u64 {
    (2..=n).fold(0, |j, i| (j + k) % i)
}
/// 1-indexed survivor for k = 2 in O(1): 2(n - 2^floor(log2 n)) + 1.
fn josephus2_closed(n: u64) -> u64 {
    let hi = 1u64 << (63 - n.leading_zeros());
    2 * (n - hi) + 1
}
fn josephus_sim(n: usize, k: usize) -> usize {
    let mut v: Vec<usize> = (0..n).collect();
    let mut pos = 0;
    while v.len() > 1 {
        pos = (pos + k - 1) % v.len();
        v.remove(pos);
    }
    v[0]
}

// ---------------------------------------------------------------- 9. Pell
fn isqrt(n: u128) -> u128 {
    let mut x = (n as f64).sqrt() as u128;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}
/// Fundamental solution of x^2 - D y^2 = 1 (D not a perfect square) via convergents of sqrt(D).
fn pell(d: u128) -> (u128, u128) {
    let a0 = isqrt(d);
    let (mut m, mut dd, mut a) = (0u128, 1u128, a0);
    let (mut h1, mut h0) = (1u128, a0); // numerators h_{-1}, h_0
    let (mut k1, mut k0) = (0u128, 1u128);
    loop {
        if (h0 * h0) as i128 - (d * k0 * k0) as i128 == 1 {
            return (h0, k0);
        }
        m = dd * a - m;
        dd = (d - m * m) / dd;
        a = (a0 + m) / dd;
        let (h2, k2) = (a * h0 + h1, a * k0 + k1);
        h1 = h0;
        h0 = h2;
        k1 = k0;
        k0 = k2;
    }
}

// ---------------------------------------------------------------- 10. perfect numbers, Collatz
fn lucas_lehmer(p: u64) -> bool {
    if p == 2 {
        return true;
    }
    let m = (1u64 << p) - 1;
    let mut s = 4u64;
    for _ in 0..p - 2 {
        s = (mulmod(s, s, m) + m - 2) % m;
    }
    s == 0
}
fn collatz_steps_naive(mut n: u64) -> u32 {
    let mut s = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        s += 1;
    }
    s
}
fn collatz_table(limit: usize) -> Vec<u32> {
    let mut cache = vec![0u32; limit];
    for start in 2..limit {
        let (mut n, mut steps) = (start as u64, 0u32);
        while n >= start as u64 {
            n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
            steps += 1;
        }
        cache[start] = steps + cache[n as usize]; // n < start is already known
    }
    cache
}

// ---------------------------------------------------------------- 11. Lucas
fn binom_lucas(mut n: u64, mut k: u64, p: u64, fact: &[u64]) -> u64 {
    let mut r = 1u64;
    while n > 0 || k > 0 {
        let (ni, ki) = (n % p, k % p);
        if ki > ni {
            return 0;
        }
        let den = mulmod(fact[ki as usize], fact[(ni - ki) as usize], p);
        r = mulmod(r, mulmod(fact[ni as usize], modinv(den, p).unwrap(), p), p);
        n /= p;
        k /= p;
    }
    r
}

fn main() {
    let mut rng = Rng::new(275);

    // ---- 1. sieves
    println!("== sieves, pi(10^7) = 664579");
    let n = 10_000_000usize;
    let t = Instant::now();
    let plain = sieve_plain(n).iter().filter(|&&b| b).count();
    println!("  plain     {plain} in {:?}", t.elapsed());
    let t = Instant::now();
    let odd = sieve_odd_count(n);
    println!("  odd-only  {odd} in {:?}", t.elapsed());
    let t = Instant::now();
    let wheel = sieve_wheel6_count(n);
    println!("  wheel-6   {wheel} in {:?}", t.elapsed());
    assert_eq!((plain, odd, wheel), (664_579, 664_579, 664_579));
    for lim in [
        0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 24, 25, 29, 30, 31, 97, 100, 1000, 9999,
    ] {
        let want = sieve_plain(lim).iter().filter(|&&b| b).count();
        assert_eq!(sieve_odd_count(lim), want, "odd {lim}");
        assert_eq!(sieve_wheel6_count(lim), want, "wheel {lim}");
    }
    let (lo, len) = (1_000_000_000_000u64, 100_000u64);
    let t = Instant::now();
    let seg = segmented_primes(lo, lo + len);
    println!(
        "  segmented [10^12, 10^12+{len}): {} primes in {:?}, first {:?}",
        seg.len(),
        t.elapsed(),
        &seg[..3]
    );
    assert_eq!(seg[0], 1_000_000_000_039); // first prime after 10^12
    assert!(seg.iter().all(|&p| is_prime(p)));
    assert_eq!(seg.len(), (lo..lo + len).filter(|&x| is_prime(x)).count());

    // ---- 2. linear sieve
    let ls = linear_sieve(1_000_000);
    println!(
        "== linear sieve: {} primes <= 10^6 (expected 78498)",
        ls.primes.len()
    );
    assert_eq!(ls.primes.len(), 78_498);
    for n in 1..=10_000usize {
        let defn = (1..=n as u64).filter(|&k| gcd(k, n as u64) == 1).count() as u32;
        assert_eq!(ls.phi[n], defn, "phi({n})");
        let f = trial_factor(n as u64);
        let mu = if f.iter().any(|&(_, e)| e > 1) {
            0
        } else if f.len() % 2 == 0 {
            1
        } else {
            -1
        };
        assert_eq!(ls.mu[n], mu, "mu({n})");
        if n > 1 {
            assert_eq!(ls.spf[n] as u64, f[0].0);
        }
    }
    // sum over d | n of phi(d) = n; sum over d | n of mu(d) = [n == 1]
    for n in 1..=2000usize {
        let (sp, sm) = (1..=n)
            .filter(|d| n % d == 0)
            .fold((0u64, 0i32), |(a, b), d| {
                (a + ls.phi[d] as u64, b + ls.mu[d] as i32)
            });
        assert_eq!(sp, n as u64);
        assert_eq!(sm, (n == 1) as i32);
    }
    println!(
        "  phi, mu, spf match the definitions for n <= 10^4; Gauss and Moebius divisor-sum identities hold"
    );

    // ---- 3. Pollard-Brent
    println!("== Miller-Rabin + Pollard-Brent");
    for n in 0..20_000u64 {
        assert_eq!(
            is_prime(n),
            n >= 2 && trial_factor(n).len() == 1 && trial_factor(n)[0].1 == 1,
            "prime {n}"
        );
    }
    assert!(!is_prime(561) && !is_prime(3_215_031_751) && is_prime((1u64 << 61) - 1));
    let t = Instant::now();
    let mut worst = 0u64;
    for _ in 0..200 {
        let (p, q) = (random_prime(&mut rng, 30), random_prime(&mut rng, 30));
        let n = p * q;
        assert!(n >> 58 != 0); // ~60 bits
        let f = factorize(n);
        assert_eq!(f.iter().product::<u64>(), n);
        assert!(f.iter().all(|&x| is_prime(x)));
        assert_eq!(f, {
            let mut e = vec![p, q];
            e.sort();
            e
        });
        worst = worst.max(n);
    }
    println!(
        "  200 random ~60-bit semiprimes split in {:?} (largest {worst})",
        t.elapsed()
    );
    let big = 2u64.pow(20) * 3u64.pow(10) * 1_000_003 * 999_983;
    assert_eq!(factorize(big).iter().product::<u64>(), big);
    assert_eq!(factorize(1_000_000_007 * 998_244_353).len(), 2);

    // ---- 4. CRT
    println!("== CRT (non-coprime)");
    assert_eq!(crt(&[(2, 3), (3, 5), (2, 7)]), Some((23, 105)));
    assert_eq!(crt(&[(1, 4), (2, 6)]), None); // parity clash mod gcd 2
    assert_eq!(crt(&[(3, 4), (5, 6)]), Some((11, 12)));
    let mut solvable = 0;
    for _ in 0..2000 {
        let eqs: Vec<(u64, u64)> = (0..3)
            .map(|_| {
                let m = 1 + rng.below(30);
                (rng.below(m), m)
            })
            .collect();
        let l = eqs.iter().fold(1, |l, &(_, m)| l / gcd(l, m) * m);
        let brute = (0..l).find(|x| eqs.iter().all(|&(a, m)| x % m == a));
        let got = crt(&eqs);
        assert_eq!(got.map(|g| g.0), brute, "{eqs:?}");
        if let Some((_, lc)) = got {
            assert_eq!(lc, l);
            solvable += 1;
        }
    }
    println!("  2000 random 3-equation systems agree with a scan ({solvable} solvable)");

    // ---- 5. BSGS
    println!("== baby-step giant-step");
    for &p in &[101u64, 1009, 10007, 65537] {
        let g = (2..p)
            .find(|&g| (1..p).all(|k| (p - 1) % k != 0 || k == p - 1 || powmod(g, k, p) != 1))
            .unwrap();
        for h in 1..p.min(400) {
            let want = (0..p - 1).find(|&x| powmod(g, x, p) == h);
            assert_eq!(bsgs(g, h, p), want, "p {p} g {g} h {h}");
        }
    }
    let p = 1_000_000_007u64;
    let t = Instant::now();
    for _ in 0..20 {
        let x = rng.below(p - 1);
        let h = powmod(5, x, p);
        let got = bsgs(5, h, p).unwrap();
        assert_eq!(powmod(5, got, p), h);
        assert!(got <= x);
    }
    println!("  20 discrete logs mod 10^9+7 in {:?}", t.elapsed());

    // ---- 6. Tonelli-Shanks
    println!("== Tonelli-Shanks");
    let small = sieve_plain(400);
    let mut checked = 0;
    for p in (3..400u64).filter(|&p| small[p as usize]) {
        let squares: Vec<bool> = {
            let mut s = vec![false; p as usize];
            for x in 0..p {
                s[(x * x % p) as usize] = true;
            }
            s
        };
        for a in 0..p {
            match tonelli_shanks(a, p) {
                Some(r) => {
                    assert_eq!(r * r % p, a);
                    assert!(squares[a as usize]);
                }
                None => assert!(!squares[a as usize]),
            }
            checked += 1;
        }
    }
    let p = (1u64 << 61) - 1;
    for _ in 0..200 {
        let x = rng.below(p);
        let a = mulmod(x, x, p);
        let r = tonelli_shanks(a, p).unwrap();
        assert!(r == x || r == p - x);
    }
    // p = 1 (mod 2^k) with big s is the hard case: 3*2^30+1 is prime
    let p = 3 * (1u64 << 30) + 1;
    assert!(is_prime(p));
    for _ in 0..100 {
        let x = rng.below(p);
        let r = tonelli_shanks(mulmod(x, x, p), p).unwrap();
        assert!(r == x || r == p - x);
    }
    println!("  {checked} (a, p) pairs vs squares table, plus 2^61-1 and 3*2^30+1 (s = 30)");

    // ---- 7. Fibonacci
    println!("== Fibonacci fast doubling");
    for m in [1u64, 2, 10, 1000, 1_000_000_007, u64::MAX] {
        for n in 0..300u64 {
            let fd = fib_doubling(n, m);
            assert_eq!(fd.0, fib_iter(n, m), "iter n {n} m {m}");
            assert_eq!(fd.0, fib_matrix(n, m), "matrix n {n} m {m}");
            assert_eq!(fd.1, fib_iter(n + 1, m));
        }
        for _ in 0..50 {
            let n = rng.next();
            assert_eq!(fib_doubling(n, m).0, fib_matrix(n, m));
        }
    }
    println!(
        "  F(10^18) mod 10^9+7 = {}",
        fib_doubling(1_000_000_000_000_000_000, 1_000_000_007).0
    );
    assert_eq!(fib_doubling(90, u64::MAX).0, 2_880_067_194_370_816_120);

    // ---- 8. Josephus
    println!("== Josephus");
    for n in 1..=300usize {
        assert_eq!(josephus2_closed(n as u64), josephus_sim(n, 2) as u64 + 1);
        for k in 1..=7usize {
            assert_eq!(josephus_rec(n as u64, k as u64), josephus_sim(n, k) as u64);
        }
    }
    assert_eq!(josephus2_closed(41), 19); // the classic: 41 people, every 2nd
    println!(
        "  J(41,2) = {} (the historical 'Josephus' seat), J(10^12, 2) = {}",
        josephus2_closed(41),
        josephus2_closed(1_000_000_000_000)
    );

    // ---- 9. Pell
    println!("== Pell x^2 - D y^2 = 1");
    assert_eq!(pell(2), (3, 2));
    assert_eq!(pell(61), (1_766_319_049, 226_153_980));
    assert_eq!(pell(109), (158_070_671_986_249, 15_140_424_455_100));
    for d in 2..=120u128 {
        if isqrt(d) * isqrt(d) == d {
            continue;
        }
        let (x, y) = pell(d);
        assert_eq!((x * x) as i128 - (d * y * y) as i128, 1);
        // minimality: no smaller y works (scan up to 20000)
        let ymax = y.min(20_000);
        for yy in 1..ymax.min(y) {
            let t = d * yy * yy + 1;
            let s = isqrt(t);
            assert!(s * s != t, "D {d}: smaller solution y={yy}");
        }
    }
    println!("  D=61: x={}, y={}", pell(61).0, pell(61).1);

    // ---- 10. perfect numbers, Collatz
    println!("== perfect numbers / Collatz");
    let mut sigma = vec![0u64; 100_001];
    for d in 1..=100_000usize {
        for m in (d..=100_000).step_by(d) {
            sigma[m] += d as u64;
        }
    }
    let naive: Vec<usize> = (2..=100_000)
        .filter(|&n| sigma[n] == 2 * n as u64)
        .collect();
    let euclid: Vec<u64> = (2..20u64)
        .filter(|&p| is_prime(p) && lucas_lehmer(p))
        .map(|p| (1u64 << (p - 1)) * ((1u64 << p) - 1))
        .filter(|&x| x <= 100_000)
        .collect();
    assert_eq!(naive, vec![6, 28, 496, 8128]);
    assert_eq!(naive.iter().map(|&x| x as u64).collect::<Vec<_>>(), euclid);
    let mersenne_exps: Vec<u64> = (2..62u64)
        .filter(|&p| is_prime(p) && lucas_lehmer(p))
        .collect();
    println!("  perfect numbers <= 10^5: {naive:?}; Mersenne exponents < 62: {mersenne_exps:?}");
    assert_eq!(mersenne_exps, vec![2, 3, 5, 7, 13, 17, 19, 31, 61]);
    let t = Instant::now();
    let table = collatz_table(1_000_000);
    let (arg, best) = (1..1_000_000usize)
        .map(|i| (i, table[i]))
        .max_by_key(|&(_, s)| s)
        .unwrap();
    println!(
        "  longest Collatz start < 10^6: {arg} with {best} steps ({:?})",
        t.elapsed()
    );
    assert_eq!((arg, best), (837_799, 524));
    for n in 1..10_000usize {
        assert_eq!(table[n], collatz_steps_naive(n as u64));
    }

    // ---- 11. Lucas
    println!("== Lucas' theorem");
    for &p in &[2u64, 3, 5, 7, 11, 13] {
        let fact: Vec<u64> = (0..p)
            .scan(1u64, |f, i| {
                if i > 0 {
                    *f = *f * i % p;
                }
                Some(*f)
            })
            .collect();
        let rows = 260usize;
        let mut pascal = vec![vec![0u64; rows + 1]; rows + 1];
        for n in 0..=rows {
            pascal[n][0] = 1 % p;
            for k in 1..=n {
                pascal[n][k] =
                    (pascal[n - 1][k - 1] + if k <= n - 1 { pascal[n - 1][k] } else { 0 }) % p;
            }
        }
        for n in 0..=rows {
            for k in 0..=n {
                assert_eq!(
                    binom_lucas(n as u64, k as u64, p, &fact),
                    pascal[n][k],
                    "C({n},{k}) mod {p}"
                );
            }
        }
    }
    let fact7 = [1u64, 1, 2, 6, 24, 120, 720];
    // Kummer-style sanity at a huge n: C(7^20, k) = 0 mod 7 unless k is 0 or 7^20
    let big = 7u64.pow(20);
    assert_eq!(binom_lucas(big, 12345, 7, &fact7), 0);
    assert_eq!(binom_lucas(big, big, 7, &fact7), 1);
    println!("  C(n,k) mod p for p in {{2,3,5,7,11,13}}, n <= 260 equals Pascal's triangle");

    println!("OK");
}
