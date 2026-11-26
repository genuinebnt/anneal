//! RECIPE 226 (crypto): secure randomness, nonces and the birthday bound, UUID v4/v7, password hashing (PBKDF2, memory-hard toy), entropy.
//! LEVEL: intermediate
//! Use in: generating keys/tokens/IDs, choosing nonce schemes, storing passwords, reviewing auth code.   Run: cargo run --release --example secure_random_and_passwords
#![allow(dead_code)]
//!
//! !!! EDUCATIONAL IMPLEMENTATION. NOT constant-time-audited, NOT for production. Use audited libraries (RustCrypto
//! !!! https://github.com/RustCrypto : `pbkdf2`, `scrypt`, `argon2`, `uuid`; ring https://github.com/briansmith/ring ) and never
//! !!! roll your own crypto for real systems. Our SHA-256/HMAC/PBKDF2 are checked against published vectors but are slow and unhardened.
//!
//! WHAT / WHY:
//!  1. RANDOMNESS: security-relevant bytes must come from the OS CSPRNG, never from `rand::thread_rng` seeded by time, a
//!     xorshift, or `RandomState`. On Unix read `/dev/urandom` (never blocks once the kernel is seeded; `/dev/random` is no
//!     "more secure" on modern kernels). The `getrandom` crate (https://docs.rs/getrandom) wraps the right syscall per OS.
//!  2. NONCES: a nonce must not repeat under one key (AES-GCM / ChaCha20-Poly1305 repeat = catastrophic). Random nonces collide
//!     by the BIRTHDAY bound p ~ 1 - exp(-n(n-1)/2N). 96-bit random nonces after 2^32 messages: p ~ 2^-33, which is why NIST SP 800-38D caps
//!     random-nonce GCM at 2^32 messages per key. We compute it and validate the formula by Monte Carlo on a 24-bit nonce.
//!     Counter nonces never collide but need durable state (and care across restarts/replicas); 192-bit random nonces (XChaCha20) make random safe.
//!  3. UUIDs (RFC 9562): v4 = 122 random bits with version/variant bits set; v7 = 48-bit Unix-ms timestamp + random, so IDs sort by
//!     creation time (great for B-tree primary keys; leaks creation time, and v4/v7 are identifiers, not secrets).
//!  4. PASSWORDS: store a slow, salted, per-user hash, never a fast hash (SHA-256 runs billions/s on a GPU). PBKDF2-HMAC-SHA256
//!     (RFC 8018) is slow by iteration count only; memory-hard functions (scrypt RFC 7914, Argon2 RFC 9106) also need lots of RAM,
//!     which makes parallel GPU/ASIC attacks expensive. We implement HMAC, PBKDF2 (RFC 7914 / well-known vectors) and a toy
//!     ROMix-style memory-hard function (SHA-256 based, NOT scrypt-compatible), verify with a constant-time compare,
//!     and print a timing table.
//!  5. STRENGTH: entropy = length * log2(pool) is an UPPER bound that holds only for uniformly random characters. Human-chosen
//!     passwords ("P@ssw0rd!") are far weaker; real estimators (zxcvbn, https://github.com/dropbox/zxcvbn) model dictionaries, keyboard
//!     walks and leetspeak. Our estimator applies only crude caps; treat it as a teaching tool.
//!
//! COMMON USES: session tokens and API keys from the OS RNG; AEAD nonce management in TLS 1.3 (sequence-number-derived nonces) and
//! libsodium-style APIs; UUIDs as primary keys; PBKDF2 in WPA2 passphrase derivation, disk-encryption key derivation and
//! many password stores; scrypt/Argon2 in modern password storage; Diceware-style passphrases.
//! Read: https://datatracker.ietf.org/doc/html/rfc9562 (UUIDs)
//! Read: https://datatracker.ietf.org/doc/html/rfc8018 (PBKDF2)
//! Read: https://datatracker.ietf.org/doc/html/rfc7914 (scrypt)
//! Read: https://datatracker.ietf.org/doc/html/rfc9106 (Argon2)
//! Read: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::sync::OnceLock;
use std::time::Instant;

// ---------------------------------------------------------------- SHA-256 (FIPS 180-4), derived constants as in merkle_tree

type Hash = [u8; 32];

fn primes(count: usize) -> Vec<u128> {
    let mut v: Vec<u128> = Vec::new();
    let mut c = 2u128;
    while v.len() < count {
        if v.iter().all(|p| c % p != 0) {
            v.push(c);
        }
        c += 1;
    }
    v
}

/// floor(cbrt(x)) by binary search; x < 2^108 so cubes fit u128.
fn icbrt(x: u128) -> u128 {
    let (mut lo, mut hi) = (0u128, 1u128 << 36);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if mid * mid * mid <= x {
            lo = mid
        } else {
            hi = mid - 1
        }
    }
    lo
}

/// The spec defines the constants as fractional bits of sqrt/cbrt of primes. Deriving them (instead of
/// pasting 72 magic numbers) removes a typo risk; the official test vectors below prove they are right.
fn constants() -> &'static ([u32; 8], [u32; 64]) {
    static C: OnceLock<([u32; 8], [u32; 64])> = OnceLock::new();
    C.get_or_init(|| {
        let ps = primes(64);
        let mut h0 = [0u32; 8];
        for (i, p) in ps.iter().take(8).enumerate() {
            h0[i] = (p << 64).isqrt() as u32; // low 32 bits = fractional part of sqrt(p) * 2^32
        }
        let mut k = [0u32; 64];
        for (i, p) in ps.iter().enumerate() {
            k[i] = icbrt(p << 96) as u32;
        }
        (h0, k)
    })
}

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let k = &constants().1;
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes(block[4 * i..4 * i + 4].try_into().unwrap());
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(k[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        (h, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

fn sha256(data: &[u8]) -> Hash {
    let mut state = constants().0;
    // Padding: 0x80, zeros up to 56 mod 64, then the message length in BITS as big-endian u64.
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks_exact(64) {
        compress(&mut state, block);
    }
    let mut out = [0u8; 32];
    for (i, s) in state.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&s.to_be_bytes());
    }
    out
}

fn hex(h: &[u8]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------- HMAC and PBKDF2

fn hmac_sha256(key: &[u8], msg: &[u8]) -> Hash {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(msg);
    let ih = sha256(&inner);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&ih);
    sha256(&outer)
}

/// PBKDF2 (RFC 8018): T_i = U_1 ^ U_2 ^ ... ^ U_c with U_1 = HMAC(P, S || INT(i)), U_j = HMAC(P, U_{j-1}).
fn pbkdf2_sha256(password: &[u8], salt: &[u8], iters: u32, dk_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(dk_len);
    let mut block = 1u32;
    while out.len() < dk_len {
        let mut s = salt.to_vec();
        s.extend_from_slice(&block.to_be_bytes());
        let mut u = hmac_sha256(password, &s);
        let mut t = u;
        for _ in 1..iters {
            u = hmac_sha256(password, &u);
            for (a, b) in t.iter_mut().zip(u) {
                *a ^= b;
            }
        }
        out.extend_from_slice(&t);
        block += 1;
    }
    out.truncate(dk_len);
    out
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && std::hint::black_box(a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y))) == 0
}

fn test_vectors() {
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    // RFC 4231 test case 2
    assert_eq!(
        hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    // Long key (> block size) path: RFC 4231 test case 6 style key of 131 x 0xaa
    assert_eq!(
        hex(&hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
    assert_eq!(
        hex(&pbkdf2_sha256(b"password", b"salt", 1, 32)),
        "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
    );
    assert_eq!(
        hex(&pbkdf2_sha256(b"password", b"salt", 2, 32)),
        "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"
    );
    assert_eq!(
        hex(&pbkdf2_sha256(b"password", b"salt", 4096, 32)),
        "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
    );
    // RFC 7914 section 11: two-block output
    assert_eq!(
        hex(&pbkdf2_sha256(b"passwd", b"salt", 1, 64)),
        "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783"
    );
    println!(
        "vectors: SHA-256, HMAC-SHA256 (RFC 4231), PBKDF2-HMAC-SHA256 (1, 2, 4096 iters; RFC 7914 64-byte case) ok"
    );
}

// ---------------------------------------------------------------- randomness, nonces

fn fill_random(buf: &mut [u8]) -> std::io::Result<()> {
    File::open("/dev/urandom")?.read_exact(buf)
}
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    fill_random(&mut b).expect(
        "OS randomness unavailable: refuse to continue rather than fall back to something weak",
    );
    b
}

/// P(at least one collision) among n uniformly random values from a space of 2^bits. Uses expm1 so tiny p stays accurate.
fn birthday(n: f64, bits: i32) -> f64 {
    let x = n * (n - 1.0) / 2.0 / 2f64.powi(bits);
    -(-x).exp_m1()
}

struct Xs(u64);
impl Xs {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn test_random_and_nonces() {
    let a: [u8; 32] = random_bytes();
    let b: [u8; 32] = random_bytes();
    assert_ne!(a, b);
    assert!(a.iter().any(|&x| x != 0));
    let ones: u32 = (0..64)
        .flat_map(|_| random_bytes::<32>())
        .map(|x| x.count_ones())
        .sum();
    assert!(
        (ones as f64 / (64.0 * 256.0) - 0.5).abs() < 0.05,
        "urandom bit balance sanity check"
    );
    println!(
        "urandom: 2 x 32 bytes differ; bit balance ~0.5 ({} random bytes sample: {}...)",
        a.len(),
        hex(&a[..4])
    );

    println!("birthday bound, random nonces:");
    for (bits, n) in [
        (96, 2f64.powi(32)),
        (96, 2f64.powi(40)),
        (128, 2f64.powi(48)),
        (192, 2f64.powi(64)),
    ] {
        println!(
            "  {bits:>3}-bit nonce, n = 2^{:<2}: p(collision) = {:.3e}",
            n.log2() as i32,
            birthday(n, bits)
        );
    }
    let p96 = birthday(2f64.powi(32), 96);
    assert!(
        (p96 / 2f64.powi(-33) - 1.0).abs() < 1e-6,
        "96-bit nonce after 2^32 messages: p = 2^-33"
    );

    // Monte Carlo on a SMALL nonce space (24 bits), n = 4096 messages: predicted p ~ 0.39.
    let (bits, n, trials) = (24, 4096usize, 4000);
    let predicted = birthday(n as f64, bits);
    let mut rng = Xs(0x9e37_79b9_7f4a_7c15);
    let mut hits = 0;
    for _ in 0..trials {
        let mut seen = HashSet::with_capacity(n);
        if (0..n).any(|_| !seen.insert(rng.next() >> (64 - bits))) {
            hits += 1;
        }
    }
    let observed = hits as f64 / trials as f64;
    println!(
        "monte carlo ({bits}-bit nonce, n={n}, {trials} trials): predicted {predicted:.4}, observed {observed:.4}"
    );
    assert!(
        (observed - predicted).abs() < 0.05,
        "formula must match simulation"
    );
    println!(
        "  counters: a 64-bit counter never collides but needs durable state; random 96-bit is fine below ~2^32 msgs/key; XChaCha's 192-bit is safe at any rate"
    );
}

// ---------------------------------------------------------------- UUIDs

fn uuid_format(b: &[u8; 16]) -> String {
    let h = hex(b);
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}
fn uuid_v4_from(mut b: [u8; 16]) -> [u8; 16] {
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant 10xx
    b
}
/// v7: 48-bit big-endian unix_ms | ver(4)=7 | rand_a(12) | var(2)=10 | rand_b(62)
fn uuid_v7_from(unix_ms: u64, rand: [u8; 10]) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[..6].copy_from_slice(&unix_ms.to_be_bytes()[2..]);
    b[6..].copy_from_slice(&rand[..10]);
    b[6] = (b[6] & 0x0f) | 0x70;
    b[8] = (b[8] & 0x3f) | 0x80;
    b
}
fn uuid_version(b: &[u8; 16]) -> u8 {
    b[6] >> 4
}
fn uuid_variant_ok(b: &[u8; 16]) -> bool {
    b[8] >> 6 == 0b10
}
fn uuid_v7_ms(b: &[u8; 16]) -> u64 {
    b[..6].iter().fold(0u64, |a, &x| (a << 8) | x as u64)
}

fn test_uuid() {
    for _ in 0..100 {
        let u = uuid_v4_from(random_bytes());
        assert_eq!(uuid_version(&u), 4);
        assert!(uuid_variant_ok(&u));
    }
    let u = uuid_v4_from(random_bytes());
    let s = uuid_format(&u);
    assert_eq!(s.len(), 36);
    assert!([8, 13, 18, 23].iter().all(|&i| s.as_bytes()[i] == b'-'));
    assert_eq!(s.as_bytes()[14], b'4');
    assert!(matches!(s.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    // fixed input: all-0xff random bits -> known layout
    assert_eq!(
        uuid_format(&uuid_v4_from([0xff; 16])),
        "ffffffff-ffff-4fff-bfff-ffffffffffff"
    );
    println!("uuid v4: {s}");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let ids: Vec<[u8; 16]> = (0..50u64)
        .map(|i| uuid_v7_from(now + i * 3, random_bytes()))
        .collect();
    for u in &ids {
        assert_eq!(uuid_version(u), 7);
        assert!(uuid_variant_ok(u));
    }
    assert!(
        ids.windows(2).all(|w| w[0] < w[1]),
        "v7 ids created at increasing ms sort lexicographically"
    );
    assert_eq!(uuid_v7_ms(&ids[10]), now + 30);
    // RFC 9562 appendix-style fixed check: timestamp 0x017F22E279B0 (1645557742000 ms) -> first 12 hex digits
    let fixed = uuid_v7_from(0x017F_22E2_79B0, [0; 10]);
    assert_eq!(&uuid_format(&fixed)[..13], "017f22e2-79b0");
    assert_eq!(uuid_format(&fixed), "017f22e2-79b0-7000-8000-000000000000");
    println!(
        "uuid v7: {} (timestamp {} ms, sortable)",
        uuid_format(&ids[0]),
        uuid_v7_ms(&ids[0])
    );
}

// ---------------------------------------------------------------- password storage

/// Stored form: pbkdf2-sha256$iters$salt_hex$hash_hex
fn hash_password(pw: &str, salt: &[u8], iters: u32) -> String {
    format!(
        "pbkdf2-sha256${iters}${}${}",
        hex(salt),
        hex(&pbkdf2_sha256(pw.as_bytes(), salt, iters, 32))
    )
}
fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}
fn verify_password(pw: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    if parts.len() != 4 || parts[0] != "pbkdf2-sha256" {
        return false;
    }
    let (Ok(iters), Some(salt), Some(want)) =
        (parts[1].parse::<u32>(), unhex(parts[2]), unhex(parts[3]))
    else {
        return false;
    };
    if iters == 0 {
        return false;
    }
    ct_eq(
        &pbkdf2_sha256(pw.as_bytes(), &salt, iters, want.len()),
        &want,
    ) // constant-time compare of the derived keys
}

fn bench_secs(f: impl FnOnce()) -> f64 {
    let t = Instant::now();
    f();
    t.elapsed().as_secs_f64()
}

fn test_passwords() {
    let salt1: [u8; 16] = random_bytes();
    let salt2: [u8; 16] = random_bytes();
    let (s1, s2) = (
        hash_password("correct horse", &salt1, 2000),
        hash_password("correct horse", &salt2, 2000),
    );
    assert_ne!(
        s1, s2,
        "per-user salts: identical passwords get different records (no rainbow tables, no equality leak)"
    );
    assert!(verify_password("correct horse", &s1));
    assert!(verify_password("correct horse", &s2));
    assert!(!verify_password("correct horsf", &s1));
    assert!(!verify_password("correct horse", "pbkdf2-sha256$0$00$00"));
    assert!(!verify_password("correct horse", "garbage"));
    assert!(
        !verify_password("correct horse", &s1.replace("2000", "2001")),
        "iteration count is part of the derivation"
    );
    println!("stored record: {s1}");

    // Why fast hashes are wrong: measure our own plain SHA-256 guess rate (a GPU is ~10^5 times faster).
    let n = 100_000u32;
    let t = bench_secs(|| {
        let mut acc = 0u8;
        for i in 0..n {
            acc ^= sha256(&i.to_le_bytes())[0];
        }
        std::hint::black_box(acc);
    });
    let rate = n as f64 / t;
    let space = 26f64.powi(8); // 8 random lowercase letters
    println!(
        "plain SHA-256 here: {:.2e} guesses/s -> all 8-letter lowercase passwords in {:.1} hours on ONE core of this machine",
        rate,
        space / rate / 3600.0
    );

    println!(
        "\nPBKDF2-HMAC-SHA256 cost grows linearly with iterations (printed; machine dependent):"
    );
    for iters in [1_000u32, 10_000, 100_000] {
        let t = bench_secs(|| {
            std::hint::black_box(pbkdf2_sha256(b"pw", b"salt", iters, 32));
        });
        println!("  iters={iters:>7}: {:>8.2} ms  (memory ~ O(1))", t * 1e3);
    }
    println!("memory-hard toy (ROMix over SHA-256; cost grows with N in time AND memory):");
    for n in [1u32 << 10, 1 << 14, 1 << 17] {
        let t = bench_secs(|| {
            std::hint::black_box(memhard(b"pw", b"salt", n));
        });
        println!(
            "  N={n:>7}: {:>8.2} ms, table = {:>6} KiB",
            t * 1e3,
            n as usize * 128 / 1024
        );
    }
}

// ---------------------------------------------------------------- memory-hard toy (scrypt-shaped, NOT scrypt)

type Blk = [u8; 128];

/// Expand/mix a 128-byte block with SHA-256 (real scrypt uses Salsa20/8 BlockMix).
fn blockmix(b: &Blk) -> Blk {
    let mut out = [0u8; 128];
    for k in 0..4u8 {
        let mut input = b.to_vec();
        input.push(k);
        out[32 * k as usize..32 * (k as usize + 1)].copy_from_slice(&sha256(&input));
    }
    out
}

/// ROMix: fill a table V of N blocks sequentially, then make N data-dependent reads. An attacker who stores less than all of V
/// must recompute entries on the fly, so time x memory stays high (this is the point of "memory-hard").
fn romix(mut x: Blk, n: u32) -> Blk {
    let mut v: Vec<Blk> = Vec::with_capacity(n as usize);
    for _ in 0..n {
        v.push(x);
        x = blockmix(&x);
    }
    for _ in 0..n {
        let j = (u64::from_le_bytes(x[..8].try_into().unwrap()) % n as u64) as usize; // secret-dependent index: not cache-timing safe (scrypt has this too; Argon2id mitigates)
        for (a, b) in x.iter_mut().zip(&v[j]) {
            *a ^= b;
        }
        x = blockmix(&x);
    }
    x
}

fn memhard(pw: &[u8], salt: &[u8], n: u32) -> Vec<u8> {
    let b: Blk = pbkdf2_sha256(pw, salt, 1, 128).try_into().unwrap();
    let x = romix(b, n);
    pbkdf2_sha256(pw, &x, 1, 32) // same shape as scrypt: PBKDF2 -> ROMix -> PBKDF2
}

fn test_memhard() {
    let a = memhard(b"pw", b"salt", 256);
    assert_eq!(a, memhard(b"pw", b"salt", 256), "deterministic");
    assert_ne!(
        a,
        memhard(b"pw", b"salt", 512),
        "N is part of the derivation"
    );
    assert_ne!(a, memhard(b"pw", b"salt2", 256));
    assert_ne!(a, memhard(b"pw2", b"salt", 256));
    println!("memhard: deterministic; depends on password, salt and N");
}

// ---------------------------------------------------------------- entropy estimator

const COMMON: [&str; 10] = [
    "password", "123456", "qwerty", "letmein", "admin", "welcome", "iloveyou", "monkey", "dragon",
    "football",
];

fn pool_size(pw: &str) -> u32 {
    let mut pool = 0;
    if pw.chars().any(|c| c.is_ascii_lowercase()) {
        pool += 26;
    }
    if pw.chars().any(|c| c.is_ascii_uppercase()) {
        pool += 26;
    }
    if pw.chars().any(|c| c.is_ascii_digit()) {
        pool += 10;
    }
    if pw.chars().any(|c| c.is_ascii_punctuation() || c == ' ') {
        pool += 33;
    }
    if pw.chars().any(|c| !c.is_ascii()) {
        pool += 100; // crude guess for "other Unicode"
    }
    pool
}

/// Naive upper bound: len * log2(pool). Valid ONLY if each character was chosen uniformly at random from the pool.
fn naive_bits(pw: &str) -> f64 {
    let pool = pool_size(pw);
    if pool == 0 {
        0.0
    } else {
        pw.chars().count() as f64 * (pool as f64).log2()
    }
}

/// Crude adjustment: undo leetspeak and check a tiny list; penalise repeats and runs. Real tools (zxcvbn) are far more thorough.
fn estimate_bits(pw: &str) -> f64 {
    let norm: String = pw
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '@' => 'a',
            '0' => 'o',
            '1' => 'l',
            '3' => 'e',
            '$' => 's',
            '5' => 's',
            c => c,
        })
        .collect();
    let stripped = norm
        .trim_end_matches(|c: char| c.is_ascii_digit() || c.is_ascii_punctuation())
        .to_string();
    if COMMON.iter().any(|w| norm == *w || stripped == *w) {
        return (COMMON.len() as f64).log2() + 4.0; // pick from the list + a little for the suffix
    }
    let chars: Vec<char> = pw.chars().collect();
    let distinct: HashSet<char> = chars.iter().copied().collect();
    let run = chars
        .windows(2)
        .filter(|w| (w[1] as i64 - w[0] as i64).abs() <= 1)
        .count();
    let mut bits = naive_bits(pw);
    if distinct.len() == 1 {
        bits = bits.min(8.0);
    }
    bits - run as f64 * (bits / chars.len().max(1) as f64) * 0.8 // each step-by-0/1 character is mostly predictable
}

fn test_entropy() {
    let cases = [
        "password",
        "P@ssw0rd!",
        "aaaaaaaa",
        "abcdefgh",
        "Tr0ub4dor&3",
        "correct horse battery staple",
        "k7#Qv9!zR2$mWx5p",
    ];
    println!(
        "\n{:<32} {:>6} {:>10} {:>10}",
        "password", "pool", "naive bits", "adjusted"
    );
    for p in cases {
        println!(
            "{p:<32} {:>6} {:>10.1} {:>10.1}",
            pool_size(p),
            naive_bits(p),
            estimate_bits(p)
        );
    }
    assert_eq!(pool_size("abc"), 26);
    assert_eq!(pool_size("aB3!"), 26 + 26 + 10 + 33);
    assert!((naive_bits("abcd") - 4.0 * 26f64.log2()).abs() < 1e-9);
    assert!(
        naive_bits("password") > 30.0 && estimate_bits("password") < 10.0,
        "naive estimate is badly wrong for dictionary words"
    );
    assert!(
        estimate_bits("P@ssw0rd!") < naive_bits("P@ssw0rd!") / 3.0,
        "leetspeak does not add real entropy"
    );
    assert!(estimate_bits("aaaaaaaa") <= 8.0);
    assert!(estimate_bits("abcdefgh") < naive_bits("abcdefgh") / 2.0);
    assert!(estimate_bits("k7#Qv9!zR2$mWx5p") > 90.0);
    // Diceware: 6 words from a 7776-word list = 6*log2(7776) ~ 77.5 bits, regardless of how "simple" the words look.
    assert!((6.0 * 7776f64.log2() - 77.5).abs() < 0.1);
    println!(
        "(caveat: these numbers are guesses about an ATTACKER's model; use zxcvbn or a breached-password check in real systems)"
    );
}

fn main() {
    println!(
        "EDUCATIONAL ONLY: not audited, not for production. Use RustCrypto (argon2/scrypt/pbkdf2/uuid), ring, getrandom.\n"
    );
    test_vectors();
    test_random_and_nonces();
    test_uuid();
    test_memhard();
    test_passwords();
    test_entropy();
    println!("\nall assertions passed");
}
