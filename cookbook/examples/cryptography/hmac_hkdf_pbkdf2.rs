//! RECIPE 220 (crypto): SHA-256 from scratch, HMAC, HKDF, PBKDF2, and the length-extension attack
//! LEVEL: advanced
//! Use in: API request signing, key derivation, password storage, TLS/JWT internals.   Run: cargo run --release --example hmac_hkdf_pbkdf2
#![allow(dead_code)]
//!
//! !!! EDUCATIONAL IMPLEMENTATION. NOT constant-time-audited. NOT FOR PRODUCTION. !!!
//! Use audited libraries (RustCrypto https://github.com/RustCrypto , ring https://github.com/briansmith/ring)
//! and never roll your own crypto for real systems. This file exists to teach how the pieces fit.
//!
//! What: SHA-256 (Merkle-Damgard, 64-byte blocks, incremental update/finalize), HMAC-SHA256 (RFC 2104/4231),
//! HKDF (RFC 5869: extract then expand), PBKDF2-HMAC-SHA256 (RFC 8018, vectors from RFC 7914 and widely published).
//! Why: SHA-256's digest IS its internal state after the last block. Anyone holding H(secret || m) can resume
//! hashing from that state and append data without knowing `secret` (LENGTH EXTENSION). So `H(key || msg)` is
//! not a MAC. HMAC = H((K^opad) || H((K^ipad) || m)): the outer hash hides the inner state, so extension fails.
//! How: the attack below rebuilds the glue padding the victim hash applied, seeds a hasher with the tag as state and
//! the right byte count, then hashes the attacker suffix. Every algorithm is asserted against official vectors.
//!
//! COMMON USES: HMAC in TLS 1.2 PRF/records and JWT HS256 signatures; HKDF in TLS 1.3 key schedule, Signal, Noise;
//! PBKDF2 for password hashing (prefer scrypt/argon2 today) and WPA2 passphrase derivation.
//! Read: https://datatracker.ietf.org/doc/html/rfc4231 , https://datatracker.ietf.org/doc/html/rfc5869 ,
//! https://datatracker.ietf.org/doc/html/rfc7914 , https://csrc.nist.gov/pubs/fips/180/4/upd1/final

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
const IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// The compression function: folds one 64-byte block into the 8-word state.
fn compress(state: &mut [u32; 8], block: &[u8]) {
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
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

/// Incremental SHA-256.
#[derive(Clone)]
struct Sha256 {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total: u64, // bytes absorbed so far
}

impl Sha256 {
    fn new() -> Self {
        Sha256 {
            state: IV,
            buf: [0; 64],
            buf_len: 0,
            total: 0,
        }
    }
    /// Resume from a known digest as if `already` bytes (a multiple of 64) had been hashed. This is the
    /// attacker's primitive in length extension: it needs no secret, only a public digest.
    fn resume(digest: &[u8; 32], already: u64) -> Self {
        assert!(already % 64 == 0);
        let mut state = [0u32; 8];
        for i in 0..8 {
            state[i] = u32::from_be_bytes(digest[4 * i..4 * i + 4].try_into().unwrap());
        }
        Sha256 {
            state,
            buf: [0; 64],
            buf_len: 0,
            total: already,
        }
    }
    fn update(&mut self, mut data: &[u8]) {
        self.total += data.len() as u64;
        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let b = self.buf;
                compress(&mut self.state, &b);
                self.buf_len = 0;
            }
        }
        while data.len() >= 64 {
            compress(&mut self.state, &data[..64]);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }
    fn finalize(mut self) -> [u8; 32] {
        let bits = self.total * 8;
        let pad = md_padding(self.total);
        self.update(&pad[..pad.len() - 8]);
        self.update(&bits.to_be_bytes());
        debug_assert_eq!(self.buf_len, 0);
        let mut out = [0u8; 32];
        for (i, s) in self.state.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&s.to_be_bytes());
        }
        out
    }
}

/// Merkle-Damgard "glue" padding for a message of `len` bytes: 0x80, zeros, 64-bit big-endian bit length.
fn md_padding(len: u64) -> Vec<u8> {
    let mut p = vec![0x80u8];
    while (len as usize + p.len()) % 64 != 56 {
        p.push(0);
    }
    p.extend_from_slice(&(len * 8).to_be_bytes());
    p
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// HMAC-SHA256: H((K' ^ opad) || H((K' ^ ipad) || msg)), K' = key zero-padded (or hashed if > 64 bytes).
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(&k.map(|b| b ^ 0x36));
    inner.update(msg);
    let ih = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&k.map(|b| b ^ 0x5c));
    outer.update(&ih);
    outer.finalize()
}

fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let zero = [0u8; 32];
    hmac_sha256(if salt.is_empty() { &zero } else { salt }, ikm)
}

fn hkdf_expand(prk: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    assert!(len <= 255 * 32);
    let mut okm = Vec::new();
    let mut t: Vec<u8> = Vec::new();
    let mut counter = 1u8;
    while okm.len() < len {
        let mut m = t.clone();
        m.extend_from_slice(info);
        m.push(counter);
        t = hmac_sha256(prk, &m).to_vec();
        okm.extend_from_slice(&t);
        counter += 1;
    }
    okm.truncate(len);
    okm
}

fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iters: u32, dk_len: usize) -> Vec<u8> {
    let mut dk = Vec::new();
    let mut block = 1u32;
    while dk.len() < dk_len {
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
        dk.extend_from_slice(&t);
        block += 1;
    }
    dk.truncate(dk_len);
    dk
}

/// Constant-time-style equality (no early exit). Still educational only.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ------------------------------------------------------------ length extension

/// The vulnerable construction: tag = SHA256(secret || msg).
fn naive_mac(secret: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(secret);
    h.update(msg);
    h.finalize()
}

/// Server side verification for the naive scheme.
fn naive_verify(secret: &[u8], msg: &[u8], tag: &[u8]) -> bool {
    ct_eq(&naive_mac(secret, msg), tag)
}

/// Attacker: knows msg, tag, and (a guess of) secret length; NOT the secret. Returns (forged_msg, forged_tag).
fn length_extend(
    msg: &[u8],
    tag: &[u8; 32],
    secret_len: usize,
    suffix: &[u8],
) -> (Vec<u8>, [u8; 32]) {
    let glue = md_padding((secret_len + msg.len()) as u64);
    let consumed = (secret_len + msg.len() + glue.len()) as u64; // multiple of 64
    let mut h = Sha256::resume(tag, consumed);
    h.update(suffix);
    let mut forged = msg.to_vec();
    forged.extend_from_slice(&glue);
    forged.extend_from_slice(suffix);
    (forged, h.finalize())
}

/// Tiny seeded xorshift for reproducible random tests.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

fn main() {
    // --- SHA-256 (FIPS 180-4 / NIST examples)
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    let mut h = Sha256::new();
    let chunk = [b'a'; 1000];
    for _ in 0..1000 {
        h.update(&chunk);
    }
    assert_eq!(
        hex(&h.finalize()),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
    println!("SHA-256 abc = {}", hex(&sha256(b"abc")));

    // Incremental == one-shot for random split points.
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..200 {
        let n = (rng.next() % 300) as usize;
        let data = rng.bytes(n);
        let mut inc = Sha256::new();
        let mut i = 0;
        while i < n {
            let step = ((rng.next() % 70) as usize + 1).min(n - i);
            inc.update(&data[i..i + step]);
            i += step;
        }
        assert_eq!(inc.finalize(), sha256(&data));
    }

    // --- HMAC-SHA256, RFC 4231
    assert_eq!(
        hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    assert_eq!(
        hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    assert_eq!(
        hex(&hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
    println!("HMAC RFC 4231 cases 1, 2, 6 OK");

    // --- HKDF, RFC 5869 test cases 1 and 3
    let ikm = [0x0b; 22];
    let salt: Vec<u8> = (0..=0x0c).collect();
    let info: Vec<u8> = (0xf0..=0xf9).collect();
    let prk = hkdf_extract(&salt, &ikm);
    assert_eq!(
        hex(&prk),
        "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
    );
    assert_eq!(
        hex(&hkdf_expand(&prk, &info, 42)),
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
    );
    let prk3 = hkdf_extract(&[], &ikm);
    assert_eq!(
        hex(&prk3),
        "19ef24a32c717b167f33a91d6f648bdf96596776afdb6377ac434c1c293ccb04"
    );
    assert_eq!(
        hex(&hkdf_expand(&prk3, &[], 42)),
        "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8"
    );
    println!("HKDF RFC 5869 cases 1, 3 OK");

    // --- PBKDF2-HMAC-SHA256
    assert_eq!(
        hex(&pbkdf2_hmac_sha256(b"passwd", b"salt", 1, 64)),
        "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783"
    );
    assert_eq!(
        hex(&pbkdf2_hmac_sha256(b"password", b"salt", 1, 32)),
        "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
    );
    assert_eq!(
        hex(&pbkdf2_hmac_sha256(b"password", b"salt", 2, 32)),
        "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"
    );
    println!("PBKDF2 vectors OK (RFC 7914 sec. 11 c=1; c=1, c=2 for password/salt)");

    // --- Length extension
    let secret = rng.bytes(16); // unknown to the attacker
    let msg = b"user=alice&amount=10";
    let tag = naive_mac(&secret, msg);
    assert!(naive_verify(&secret, msg, &tag));
    // Attacker guesses secret length (tries 1..=64 in practice) and appends "&amount=1000000".
    let (forged_msg, forged_tag) = length_extend(msg, &tag, secret.len(), b"&amount=1000000");
    assert!(
        naive_verify(&secret, &forged_msg, &forged_tag),
        "length extension must forge a valid tag"
    );
    assert_ne!(&forged_msg[..], &msg[..]);
    println!(
        "LENGTH EXTENSION: forged msg ({} bytes) accepted by naive H(secret||msg)",
        forged_msg.len()
    );
    // The same trick against HMAC: the attacker's forged tag does not verify.
    let htag = hmac_sha256(&secret, msg);
    let htag_arr: [u8; 32] = htag;
    let (hforged, hforged_tag) = length_extend(msg, &htag_arr, secret.len(), b"&amount=1000000");
    assert!(
        !ct_eq(&hmac_sha256(&secret, &hforged), &hforged_tag),
        "HMAC must resist length extension"
    );
    println!("HMAC rejects the same forgery (outer hash hides the inner state)");

    // Random HKDF/PBKDF2 sanity: determinism and length.
    let d1 = pbkdf2_hmac_sha256(b"pw", b"salt", 10, 70);
    assert_eq!(d1, pbkdf2_hmac_sha256(b"pw", b"salt", 10, 70));
    assert_eq!(&d1[..32], &pbkdf2_hmac_sha256(b"pw", b"salt", 10, 32)[..]);
    assert_eq!(unhex("0aff"), vec![0x0a, 0xff]);
    println!("all r220 checks passed");
}
