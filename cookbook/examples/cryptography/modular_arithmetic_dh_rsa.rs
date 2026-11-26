//! RECIPE 223 (crypto): modular arithmetic, Miller-Rabin, Diffie-Hellman, toy RSA (+CRT, malleability), Shamir secret sharing.
//! LEVEL: advanced
//! Use in: understanding public-key crypto, number theory code, secret splitting.   Run: cargo run --release --example modular_arithmetic_dh_rsa
#![allow(dead_code)]
//!
//! !!! EDUCATIONAL IMPLEMENTATION. NOT constant-time-audited, NOT for production. Parameters here are tiny on purpose
//! !!! (they are breakable in milliseconds). For real systems use audited libraries (RustCrypto https://github.com/RustCrypto ,
//! !!! ring https://github.com/briansmith/ring) and NEVER roll your own crypto.
//!
//! WHAT: the number theory under public-key crypto, in ~400 lines of std-only Rust.
//!  1. `mulmod` via u128 (a*b never overflows for 64-bit moduli), square-and-multiply `powmod`, extended Euclid and `modinv`.
//!  2. Miller-Rabin, DETERMINISTIC for all u64 with the first 12 primes as witnesses (proven sufficient below 3.3e24);
//!     checked against a sieve for every n < 100000 and against known tricky composites (Carmichael 561, strong pseudoprimes).
//!  3. Diffie-Hellman over a safe prime p = 2q+1 with a generator of the order-q subgroup. Both sides get g^(ab).
//!     A 22-bit group is broken here by brute-force discrete log: the security is ONLY the size of the group.
//!  4. Toy RSA: textbook example (61, 53), then ~62-bit moduli from our own prime generator; sign/verify; CRT decryption
//!     (two half-size exponentiations + Garner recombination, ~3-4x faster in real sizes).
//!  5. TEXTBOOK RSA is deterministic (same message -> same ciphertext, so equality leaks) and MALLEABLE
//!     (E(m1)*E(m2) = E(m1*m2)). Real RSA needs padding: OAEP for encryption, PSS for signatures (RFC 8017).
//!  6. Shamir secret sharing over GF(p), p = 2^61-1: a random degree k-1 polynomial with f(0)=secret; share i is (i, f(i)).
//!     Any k shares reconstruct by Lagrange interpolation at 0; k-1 shares are consistent with EVERY possible secret
//!     (information-theoretic security), which we demonstrate by constructing a valid polynomial for a wrong guess.
//!
//! COMMON USES: Miller-Rabin in key generation (RSA/DH primes); Diffie-Hellman (finite-field and its elliptic-curve variant)
//! in TLS and SSH key exchange; RSA in X.509 certificates and PKCS#1 (with OAEP/PSS padding); CRT optimisation inside RSA
//! libraries; Shamir sharing for splitting master keys (e.g. HashiCorp Vault unseal keys, SLIP-39 wallet backups).
//! Read: https://www.rfc-editor.org/rfc/rfc8017 (PKCS#1 v2.2: RSA, OAEP, PSS)
//! Read: https://www.rfc-editor.org/rfc/rfc7919 (named finite-field DH groups for TLS)
//! Read: https://en.wikipedia.org/wiki/Miller%E2%80%93Rabin_primality_test
//! Read: https://dl.acm.org/doi/10.1145/359168.359176 (Shamir, "How to share a secret", 1979)

// ---------------------------------------------------------------- modular arithmetic

fn mulmod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}
fn addmod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 + b as u128) % m as u128) as u64
}
fn submod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 + m as u128 - (b % m) as u128) % m as u128) as u64
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

/// Extended Euclid: returns (g, x, y) with a*x + b*y = g = gcd(a, b).
fn egcd(a: i128, b: i128) -> (i128, i128, i128) {
    let (mut r0, mut r1, mut s0, mut s1, mut t0, mut t1) = (a, b, 1i128, 0i128, 0i128, 1i128);
    while r1 != 0 {
        let q = r0 / r1;
        (r0, r1) = (r1, r0 - q * r1);
        (s0, s1) = (s1, s0 - q * s1);
        (t0, t1) = (t1, t0 - q * t1);
    }
    (r0, s0, t0)
}
fn modinv(a: u64, m: u64) -> Option<u64> {
    let (g, x, _) = egcd(a as i128, m as i128);
    (g == 1).then(|| x.rem_euclid(m as i128) as u64)
}
fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

// ---------------------------------------------------------------- Miller-Rabin

const WITNESSES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];

fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    for p in WITNESSES {
        if n == p {
            return true;
        }
        if n % p == 0 {
            return false;
        }
    }
    let s = (n - 1).trailing_zeros();
    let d = (n - 1) >> s; // n-1 = d * 2^s with d odd
    'witness: for a in WITNESSES {
        let mut x = powmod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mulmod(x, x, n);
            if x == n - 1 {
                continue 'witness;
            }
        }
        return false; // a proves n composite
    }
    true
}

/// Tiny deterministic PRNG (xorshift64*). NOT cryptographic: fine for a reproducible demo only.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Random prime with exactly `bits` bits (top bit set so p*q has a predictable size).
fn gen_prime(rng: &mut Rng, bits: u32) -> u64 {
    loop {
        let c = (rng.next() >> (64 - bits)) | (1 << (bits - 1)) | 1;
        if is_prime(c) {
            return c;
        }
    }
}

fn test_primes() {
    const N: usize = 100_000;
    let mut sieve = vec![true; N];
    sieve[0] = false;
    sieve[1] = false;
    let mut i = 2;
    while i * i < N {
        if sieve[i] {
            (i * i..N).step_by(i).for_each(|j| sieve[j] = false);
        }
        i += 1;
    }
    for n in 0..N {
        assert_eq!(is_prime(n as u64), sieve[n], "n={n}");
    }
    assert!(!is_prime(561)); // smallest Carmichael number fools the plain Fermat test
    assert!(!is_prime(3_215_031_751)); // strong pseudoprime to bases 2,3,5,7
    assert!(!is_prime(3_825_123_056_546_413_051)); // strong pseudoprime to the first 9 primes; our 12 catch it
    assert!(is_prime((1 << 61) - 1)); // Mersenne prime
    assert!(is_prime(18_446_744_073_709_551_557)); // 2^64 - 59, the largest u64 prime
    assert!(!is_prime(u64::MAX));
    println!(
        "miller-rabin: agrees with sieve for all n < {N}; Carmichael/pseudoprimes rejected; 2^64-59 accepted"
    );
}

fn test_arith() {
    assert_eq!(mulmod(u64::MAX - 1, u64::MAX - 2, u64::MAX), 2); // would overflow in u64
    assert_eq!(powmod(2, 10, 1000), 24);
    let p = 1_000_000_007;
    assert_eq!(powmod(123456789, p - 1, p), 1); // Fermat's little theorem
    let inv = modinv(3, 11).unwrap();
    assert_eq!(inv, 4);
    assert!(modinv(6, 9).is_none());
    let (g, x, y) = egcd(240, 46);
    assert_eq!((g, 240 * x + 46 * y), (2, 2));
    for a in 1..200u64 {
        if let Some(i) = modinv(a, 1009) {
            assert_eq!(mulmod(a, i, 1009), 1);
        }
    }
    println!("arith: mulmod/powmod/egcd/modinv ok");
}

// ---------------------------------------------------------------- Diffie-Hellman

/// Smallest safe prime p = 2q+1 (q prime) at or above `start`.
fn safe_prime_from(start: u64) -> (u64, u64) {
    let mut q = start | 1;
    loop {
        if is_prime(q) && is_prime(2 * q + 1) {
            return (2 * q + 1, q);
        }
        q += 2;
    }
}

/// Generator of the order-q subgroup: any square (other than 1) has order q because |Z_p*| = 2q.
fn subgroup_generator(p: u64, q: u64) -> u64 {
    (2..)
        .map(|h| mulmod(h, h, p))
        .find(|&g| g != 1 && powmod(g, q, p) == 1)
        .unwrap()
}

fn test_dh() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    // A 61-bit-ish group: both sides agree.
    let (p, q) = safe_prime_from(1 << 60);
    let g = subgroup_generator(p, q);
    let (a, b) = (1 + rng.below(q - 1), 1 + rng.below(q - 1)); // private exponents in [1, q-1]
    let (pa, pb) = (powmod(g, a, p), powmod(g, b, p)); // public values sent over the wire
    let (sa, sb) = (powmod(pb, a, p), powmod(pa, b, p));
    assert_eq!(sa, sb);
    println!("DH p={p} g={g}: A={pa} B={pb} shared={sa}");

    // A tiny group: an eavesdropper who sees only p, g, A, B recovers the secret by brute force.
    let (p, q) = safe_prime_from(1 << 21);
    let g = subgroup_generator(p, q);
    let (a, b) = (1 + rng.below(q - 1), 1 + rng.below(q - 1));
    let (pa, pb) = (powmod(g, a, p), powmod(g, b, p));
    let secret = powmod(pb, a, p);
    let (mut x, mut steps) = (1u64, 0u64);
    while x != pa {
        x = mulmod(x, g, p); // try g^1, g^2, ... until we hit A
        steps += 1;
    }
    let recovered_a = steps; // x = g^steps
    let eve_secret = powmod(pb, recovered_a, p);
    assert_eq!(eve_secret, secret);
    println!(
        "brute-force dlog on a {}-bit group: {} steps -> eavesdropper derives the shared secret {}",
        64 - p.leading_zeros(),
        steps,
        eve_secret
    );
    println!(
        "  (real DH uses >= 2048-bit groups, e.g. RFC 7919 ffdhe2048, or elliptic curves like X25519)"
    );
}

// ---------------------------------------------------------------- RSA

struct RsaKey {
    n: u64,
    e: u64,
    d: u64,
    p: u64,
    q: u64,
}

fn rsa_from_primes(p: u64, q: u64, e: u64) -> Option<RsaKey> {
    let lambda = (p - 1) / gcd(p - 1, q - 1) * (q - 1); // Carmichael lambda(n) = lcm(p-1, q-1)
    let d = modinv(e, lambda)?;
    Some(RsaKey {
        n: p * q,
        e,
        d,
        p,
        q,
    })
}

fn rsa_keygen(rng: &mut Rng) -> RsaKey {
    loop {
        let (p, q) = (gen_prime(rng, 31), gen_prime(rng, 31));
        if p != q {
            if let Some(k) = rsa_from_primes(p, q, 65537) {
                return k;
            }
        }
    }
}

fn rsa_enc(m: u64, n: u64, e: u64) -> u64 {
    powmod(m, e, n)
}
fn rsa_dec(c: u64, k: &RsaKey) -> u64 {
    powmod(c, k.d, k.n)
}
/// CRT decryption (Garner): m = m2 + q * (qinv * (m1 - m2) mod p).
fn rsa_dec_crt(c: u64, k: &RsaKey) -> u64 {
    let (dp, dq) = (k.d % (k.p - 1), k.d % (k.q - 1));
    let qinv = modinv(k.q % k.p, k.p).unwrap();
    let m1 = powmod(c % k.p, dp, k.p);
    let m2 = powmod(c % k.q, dq, k.q);
    let h = mulmod(qinv, submod(m1, m2 % k.p, k.p), k.p);
    m2 + h * k.q
}

/// Toy digest (FNV-1a) reduced mod n. A real scheme hashes with SHA-256 and pads with PSS.
fn toy_hash(msg: &[u8], n: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in msg {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h % n
}
fn rsa_sign(msg: &[u8], k: &RsaKey) -> u64 {
    powmod(toy_hash(msg, k.n), k.d, k.n)
}
fn rsa_verify(msg: &[u8], sig: u64, n: u64, e: u64) -> bool {
    powmod(sig, e, n) == toy_hash(msg, n)
}

fn test_rsa() {
    // The classic textbook example.
    let k = rsa_from_primes(61, 53, 17).unwrap();
    assert_eq!((k.n, k.d), (3233, 413)); // with lambda=780 -> d=413 (phi=3120 gives the better-known d=2753)
    assert_eq!(modinv(17, 3120), Some(2753));
    let c = rsa_enc(65, 3233, 17);
    assert_eq!(c, 2790);
    assert_eq!(powmod(c, 2753, 3233), 65);
    assert_eq!(rsa_dec(c, &k), 65);
    println!(
        "textbook RSA n=3233 e=17: 65 -> {c} -> 65 (d=2753 via phi, d=413 via lambda: both work)"
    );

    let mut rng = Rng(0xdead_beef_cafe_f00d);
    let k = rsa_keygen(&mut rng);
    assert!(k.n >> 60 != 0, "modulus should be ~62 bits");
    println!(
        "keygen: p={} q={} n={} ({} bits) e={} d={}",
        k.p,
        k.q,
        k.n,
        64 - k.n.leading_zeros(),
        k.e,
        k.d
    );
    for _ in 0..200 {
        let m = rng.below(k.n);
        let c = rsa_enc(m, k.n, k.e);
        assert_eq!(rsa_dec(c, &k), m);
        assert_eq!(rsa_dec_crt(c, &k), m, "CRT must equal plain decryption");
    }
    println!("200 random messages: encrypt/decrypt and CRT decrypt agree");

    let msg = b"pay alice 10";
    let sig = rsa_sign(msg, &k);
    assert!(rsa_verify(msg, sig, k.n, k.e));
    assert!(!rsa_verify(b"pay alice 99", sig, k.n, k.e));
    assert!(!rsa_verify(msg, sig ^ 1, k.n, k.e));
    println!("sign/verify ok; tampered message and tampered signature rejected");

    // Deterministic: an observer can confirm a guess by encrypting it.
    let (yes, no) = (1u64, 0u64);
    let c_yes = rsa_enc(yes, k.n, k.e);
    assert_eq!(c_yes, rsa_enc(yes, k.n, k.e));
    assert_ne!(c_yes, rsa_enc(no, k.n, k.e));
    // Malleable: multiply ciphertexts, get the product of plaintexts without the key.
    let (m1, m2) = (123_456u64, 789u64);
    let (c1, c2) = (rsa_enc(m1, k.n, k.e), rsa_enc(m2, k.n, k.e));
    let forged = mulmod(c1, c2, k.n);
    assert_eq!(rsa_dec(forged, &k), mulmod(m1, m2, k.n));
    let doubled = mulmod(c1, rsa_enc(2, k.n, k.e), k.n); // attacker doubles a hidden amount
    assert_eq!(rsa_dec(doubled, &k), 2 * m1);
    println!(
        "textbook RSA: deterministic (guess-and-check works) and malleable (E(m1)*E(m2) decrypts to m1*m2, E(m1)*E(2) to 2*m1)"
    );
    println!(
        "  fix: OAEP padding (randomised, integrity-checked) for encryption, PSS for signatures"
    );
}

// ---------------------------------------------------------------- Shamir secret sharing over GF(2^61 - 1)

const P: u64 = (1 << 61) - 1;

fn eval_poly(coef: &[u64], x: u64) -> u64 {
    coef.iter()
        .rev()
        .fold(0, |acc, &c| addmod(mulmod(acc, x, P), c, P)) // Horner
}

fn split(secret: u64, k: usize, n: usize, rng: &mut Rng) -> Vec<(u64, u64)> {
    let mut coef = vec![secret % P];
    coef.extend((1..k).map(|_| rng.below(P)));
    (1..=n as u64).map(|x| (x, eval_poly(&coef, x))).collect()
}

/// Lagrange interpolation: value at `x` of the unique degree < points.len() polynomial through `points`.
fn lagrange(points: &[(u64, u64)], x: u64) -> u64 {
    let mut total = 0;
    for (i, &(xi, yi)) in points.iter().enumerate() {
        let (mut num, mut den) = (1u64, 1u64);
        for (j, &(xj, _)) in points.iter().enumerate() {
            if i != j {
                num = mulmod(num, submod(x, xj, P), P);
                den = mulmod(den, submod(xi, xj, P), P);
            }
        }
        total = addmod(
            total,
            mulmod(yi, mulmod(num, modinv(den, P).unwrap(), P), P),
            P,
        );
    }
    total
}

fn test_shamir() {
    let mut rng = Rng(42);
    let secret = 1_234_567_890_123_456_789 % P;
    let (k, n) = (3, 5);
    let shares = split(secret, k, n, &mut rng);
    let mut combos = 0;
    for a in 0..n {
        for b in a + 1..n {
            for c in b + 1..n {
                assert_eq!(lagrange(&[shares[a], shares[b], shares[c]], 0), secret);
                combos += 1;
            }
        }
    }
    assert_eq!(combos, 10);
    println!("shamir (k={k}, n={n}): all {combos} subsets of {k} shares reconstruct the secret");

    // k-1 = 2 shares: interpolating them WITH any guessed secret gives a valid degree-2 polynomial.
    let two = [shares[0], shares[3]];
    for guess in [0u64, 7, secret ^ 1, P - 1] {
        let pts = [(0, guess), two[0], two[1]];
        assert_eq!(lagrange(&pts, two[0].0), two[0].1);
        assert_eq!(lagrange(&pts, two[1].0), two[1].1);
        assert_eq!(lagrange(&pts, 0), guess);
    }
    // Naively "reconstructing" from 2 shares gives a wrong value (with overwhelming probability).
    assert_ne!(lagrange(&two, 0), secret);
    println!(
        "with only {} shares every guessed secret fits some polynomial: no information leaks",
        k - 1
    );
}

fn main() {
    println!(
        "EDUCATIONAL ONLY: toy sizes, not constant-time, not for production. Use RustCrypto / ring.\n"
    );
    test_arith();
    test_primes();
    test_dh();
    test_rsa();
    test_shamir();
    println!("\nall assertions passed");
}
