//! RECIPE 221 (crypto): ChaCha20, Poly1305 and the RFC 8439 AEAD, plus nonce-reuse and tamper demos
//! LEVEL: advanced
//! Use in: TLS 1.3 / SSH / WireGuard-style record protection, encrypted files and tokens.   Run: cargo run --release --example chacha20_poly1305
#![allow(dead_code)]
//!
//! !!! EDUCATIONAL IMPLEMENTATION. NOT constant-time-audited. NOT FOR PRODUCTION. !!!
//! Use audited libraries (RustCrypto https://github.com/RustCrypto , ring https://github.com/briansmith/ring)
//! and never roll your own crypto for real systems. This file exists to teach how the pieces fit.
//!
//! What: ChaCha20 (ARX stream cipher: add, rotate, xor on a 4x4 u32 state), Poly1305 (one-time polynomial MAC
//! over the prime 2^130-5) and their AEAD composition (RFC 8439 section 2.8).
//! Why AEAD: encryption alone gives confidentiality but no integrity; an attacker can flip ciphertext bits and
//! flip the same plaintext bits. The tag authenticates ciphertext AND associated data (AAD, e.g. headers).
//! How Poly1305 arithmetic works here: 130-bit numbers are held in five 26-bit limbs inside u64s, so a limb
//! product (<2^52) summed five times never overflows u64. Reduction uses 2^130 = 5 (mod p), hence the "*5" terms.
//! The one-time Poly1305 key is the first 32 bytes of the ChaCha20 block with counter 0; the message is
//! encrypted starting at counter 1. Decrypt verifies the tag FIRST and returns no plaintext on failure.
//! NONCE REUSE is catastrophic: same key+nonce means the same keystream, so c1 ^ c2 == p1 ^ p2, and the
//! Poly1305 key repeats too, which enables forgeries. Nonces must be unique per key (counter or random 96-bit w/ care).
//!
//! COMMON USES: TLS 1.3 cipher suite TLS_CHACHA20_POLY1305_SHA256, OpenSSH chacha20-poly1305@openssh.com,
//! WireGuard data packets (ChaCha20-Poly1305), mobile devices without AES hardware.
//! Read: https://datatracker.ietf.org/doc/html/rfc8439

fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}

/// ChaCha20 block function: 20 rounds (10 column+diagonal double rounds), then add the input state.
fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut init = [0u32; 16];
    init[..4].copy_from_slice(&[0x61707865, 0x3320646e, 0x79622d32, 0x6b206574]); // "expand 32-byte k"
    for i in 0..8 {
        init[4 + i] = le32(&key[4 * i..]);
    }
    init[12] = counter;
    for i in 0..3 {
        init[13 + i] = le32(&nonce[4 * i..]);
    }
    let mut s = init;
    for _ in 0..10 {
        quarter_round(&mut s, 0, 4, 8, 12);
        quarter_round(&mut s, 1, 5, 9, 13);
        quarter_round(&mut s, 2, 6, 10, 14);
        quarter_round(&mut s, 3, 7, 11, 15);
        quarter_round(&mut s, 0, 5, 10, 15);
        quarter_round(&mut s, 1, 6, 11, 12);
        quarter_round(&mut s, 2, 7, 8, 13);
        quarter_round(&mut s, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        out[4 * i..4 * i + 4].copy_from_slice(&s[i].wrapping_add(init[i]).to_le_bytes());
    }
    out
}

/// Encrypt/decrypt (same operation): XOR with the keystream starting at `counter`.
fn chacha20_xor(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for (i, chunk) in data.chunks(64).enumerate() {
        let ks = chacha20_block(key, counter.wrapping_add(i as u32), nonce);
        out.extend(chunk.iter().zip(ks).map(|(d, k)| d ^ k));
    }
    out
}

/// Poly1305 one-time MAC with 5x26-bit limbs.
fn poly1305(key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
    const M: u64 = 0x3ff_ffff;
    let l = |o: usize| le32(&key[o..]) as u64;
    // Clamp r (the masks clear the bits RFC 8439 requires to be zero).
    let r = [
        l(0) & 0x3ff_ffff,
        (l(3) >> 2) & 0x3ffff03,
        (l(6) >> 4) & 0x3ffc0ff,
        (l(9) >> 6) & 0x3f03fff,
        (l(12) >> 8) & 0x00fffff,
    ];
    let s5 = [r[1] * 5, r[2] * 5, r[3] * 5, r[4] * 5];
    let mut h = [0u64; 5];
    for chunk in msg.chunks(16) {
        let mut b = [0u8; 20];
        b[..chunk.len()].copy_from_slice(chunk);
        let hibit = if chunk.len() == 16 {
            1u64 << 24
        } else {
            b[chunk.len()] = 1; // partial block: append 0x01, no high bit
            0
        };
        let w = |o: usize| le32(&b[o..]) as u64;
        h[0] += w(0) & M;
        h[1] += (w(3) >> 2) & M;
        h[2] += (w(6) >> 4) & M;
        h[3] += (w(9) >> 6) & M;
        h[4] += (w(12) >> 8) | hibit;
        let d0 = h[0] * r[0] + h[1] * s5[3] + h[2] * s5[2] + h[3] * s5[1] + h[4] * s5[0];
        let d1 = h[0] * r[1] + h[1] * r[0] + h[2] * s5[3] + h[3] * s5[2] + h[4] * s5[1];
        let d2 = h[0] * r[2] + h[1] * r[1] + h[2] * r[0] + h[3] * s5[3] + h[4] * s5[2];
        let d3 = h[0] * r[3] + h[1] * r[2] + h[2] * r[1] + h[3] * r[0] + h[4] * s5[3];
        let d4 = h[0] * r[4] + h[1] * r[3] + h[2] * r[2] + h[3] * r[1] + h[4] * r[0];
        let mut c = d0 >> 26;
        h[0] = d0 & M;
        let d1 = d1 + c;
        c = d1 >> 26;
        h[1] = d1 & M;
        let d2 = d2 + c;
        c = d2 >> 26;
        h[2] = d2 & M;
        let d3 = d3 + c;
        c = d3 >> 26;
        h[3] = d3 & M;
        let d4 = d4 + c;
        c = d4 >> 26;
        h[4] = d4 & M;
        h[0] += c * 5;
        c = h[0] >> 26;
        h[0] &= M;
        h[1] += c;
    }
    // Full carry, then conditionally subtract p = 2^130 - 5.
    let mut c;
    c = h[1] >> 26;
    h[1] &= M;
    h[2] += c;
    c = h[2] >> 26;
    h[2] &= M;
    h[3] += c;
    c = h[3] >> 26;
    h[3] &= M;
    h[4] += c;
    c = h[4] >> 26;
    h[4] &= M;
    h[0] += c * 5;
    c = h[0] >> 26;
    h[0] &= M;
    h[1] += c;
    let mut g = [0u64; 5];
    g[0] = h[0] + 5;
    c = g[0] >> 26;
    g[0] &= M;
    for i in 1..4 {
        g[i] = h[i] + c;
        c = g[i] >> 26;
        g[i] &= M;
    }
    g[4] = (h[4] + c).wrapping_sub(1 << 26);
    let mask = (g[4] >> 63).wrapping_sub(1); // all ones if h >= p (no underflow)
    for i in 0..5 {
        h[i] = (h[i] & !mask) | (g[i] & mask);
    }
    let h128 = (h[0] as u128)
        | ((h[1] as u128) << 26)
        | ((h[2] as u128) << 52)
        | ((h[3] as u128) << 78)
        | ((h[4] as u128) << 104);
    let pad = u128::from_le_bytes(key[16..32].try_into().unwrap());
    h128.wrapping_add(pad).to_le_bytes()
}

/// Constant-time-style comparison: accumulate differences, no early exit.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn aead_tag(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ct: &[u8]) -> [u8; 16] {
    let otk: [u8; 32] = chacha20_block(key, 0, nonce)[..32].try_into().unwrap();
    let mut m = Vec::new();
    m.extend_from_slice(aad);
    m.resize(m.len().next_multiple_of(16), 0);
    m.extend_from_slice(ct);
    m.resize(m.len().next_multiple_of(16), 0);
    m.extend_from_slice(&(aad.len() as u64).to_le_bytes());
    m.extend_from_slice(&(ct.len() as u64).to_le_bytes());
    poly1305(&otk, &m)
}

fn aead_encrypt(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], pt: &[u8]) -> (Vec<u8>, [u8; 16]) {
    let ct = chacha20_xor(key, 1, nonce, pt);
    let tag = aead_tag(key, nonce, aad, &ct);
    (ct, tag)
}

#[derive(Debug, PartialEq)]
struct AuthError;

/// Verifies the tag before touching the ciphertext with the cipher: no plaintext is released on failure.
fn aead_decrypt(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    ct: &[u8],
    tag: &[u8; 16],
) -> Result<Vec<u8>, AuthError> {
    let expect = aead_tag(key, nonce, aad, ct);
    if !ct_eq(&expect, tag) {
        return Err(AuthError);
    }
    Ok(chacha20_xor(key, 1, nonce, ct))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    let s: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ':')
        .collect();
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

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

const SUNSCREEN: &[u8] = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

fn main() {
    // RFC 8439 2.1.1 quarter round
    let mut s = [0u32; 16];
    s[0] = 0x11111111;
    s[1] = 0x01020304;
    s[2] = 0x9b8d6f43;
    s[3] = 0x01234567;
    quarter_round(&mut s, 0, 1, 2, 3);
    assert_eq!(
        [s[0], s[1], s[2], s[3]],
        [0xea2a92f4, 0xcb1cf8ce, 0x4581472e, 0x5881c4bb]
    );

    // 2.3.2 block function
    let key: [u8; 32] = core::array::from_fn(|i| i as u8);
    let nonce: [u8; 12] = [0, 0, 0, 9, 0, 0, 0, 0x4a, 0, 0, 0, 0];
    let blk = chacha20_block(&key, 1, &nonce);
    assert_eq!(
        hex(&blk),
        "10f1e7e4d13b5915500fdd1fa32071c4c7d1f4c733c068030422aa9ac3d46c4ed2826446079faa0914c2d705d98b02a2b5129cd1de164eb9cbd083e8a2503c4e"
    );
    println!("ChaCha20 block OK: {}...", &hex(&blk)[..16]);

    // 2.4.2 encryption
    let nonce2: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0x4a, 0, 0, 0, 0];
    let ct = chacha20_xor(&key, 1, &nonce2, SUNSCREEN);
    assert_eq!(
        hex(&ct),
        "6e2e359a2568f98041ba0728dd0d6981e97e7aec1d4360c20a27afccfd9fae0bf91b65c5524733ab8f593dabcd62b3571639d624e65152ab8f530c359f0861d807ca0dbf500d6a6156a38e088a22b65e52bc514d16ccf806818ce91ab77937365af90bbf74a35be6b40b8eedf2785e42874d"
    );
    assert_eq!(chacha20_xor(&key, 1, &nonce2, &ct), SUNSCREEN);
    println!("ChaCha20 encryption OK (114 bytes)");

    // 2.5.2 Poly1305
    let pk: [u8; 32] = unhex("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b")
        .try_into()
        .unwrap();
    assert_eq!(
        hex(&poly1305(&pk, b"Cryptographic Forum Research Group")),
        "a8061dc1305136c6c22b8baf0c0127a9"
    );
    println!("Poly1305 OK");

    // 2.8.2 AEAD
    let akey: [u8; 32] = core::array::from_fn(|i| 0x80 + i as u8);
    let anonce: [u8; 12] = unhex("070000004041424344454647").try_into().unwrap();
    let aad = unhex("50515253c0c1c2c3c4c5c6c7");
    let (act, atag) = aead_encrypt(&akey, &anonce, &aad, SUNSCREEN);
    assert_eq!(
        hex(&act),
        "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b6116"
    );
    assert_eq!(hex(&atag), "1ae10b594f09e26a7e902ecbd0600691");
    assert_eq!(
        aead_decrypt(&akey, &anonce, &aad, &act, &atag).unwrap(),
        SUNSCREEN
    );
    println!("AEAD RFC 8439 2.8.2 OK, tag = {}", hex(&atag));

    // Tampering with ciphertext, AAD, tag, or nonce is rejected.
    let mut bad_ct = act.clone();
    bad_ct[5] ^= 1;
    assert_eq!(
        aead_decrypt(&akey, &anonce, &aad, &bad_ct, &atag),
        Err(AuthError)
    );
    let mut bad_aad = aad.clone();
    bad_aad[0] ^= 0x80;
    assert_eq!(
        aead_decrypt(&akey, &anonce, &bad_aad, &act, &atag),
        Err(AuthError)
    );
    let mut bad_tag = atag;
    bad_tag[15] ^= 1;
    assert_eq!(
        aead_decrypt(&akey, &anonce, &aad, &act, &bad_tag),
        Err(AuthError)
    );
    let mut bad_nonce = anonce;
    bad_nonce[0] ^= 1;
    assert_eq!(
        aead_decrypt(&akey, &bad_nonce, &aad, &act, &atag),
        Err(AuthError)
    );
    assert_eq!(
        aead_decrypt(&akey, &anonce, &aad, &act[..act.len() - 1], &atag),
        Err(AuthError)
    );
    println!("tampering with ct / aad / tag / nonce / truncation: all rejected");

    // Nonce reuse: identical keystream leaks the XOR of the plaintexts.
    let p1 = b"attack at dawn!!";
    let p2 = b"retreat at noon!";
    let (c1, t1) = aead_encrypt(&akey, &anonce, b"", p1);
    let (c2, t2) = aead_encrypt(&akey, &anonce, b"", p2);
    let cx: Vec<u8> = c1.iter().zip(&c2).map(|(a, b)| a ^ b).collect();
    let px: Vec<u8> = p1.iter().zip(p2).map(|(a, b)| a ^ b).collect();
    assert_eq!(cx, px, "same key+nonce: c1^c2 == p1^p2");
    // Knowing p1 (a "crib"), the attacker recovers p2 without the key.
    let recovered: Vec<u8> = cx.iter().zip(p1).map(|(a, b)| a ^ b).collect();
    assert_eq!(&recovered[..], &p2[..]);
    assert_ne!(t1, t2);
    println!(
        "NONCE REUSE: c1^c2 == p1^p2, recovered {:?} from a known plaintext",
        String::from_utf8_lossy(&recovered)
    );

    // Seeded random round trips, with a random single-bit ciphertext flip that must be rejected.
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    for n in 0..200usize {
        let k: [u8; 32] = rng.bytes(32).try_into().unwrap();
        let nn: [u8; 12] = rng.bytes(12).try_into().unwrap();
        let ad = rng.bytes(n % 37);
        let pt = rng.bytes(n * 3);
        let (c, t) = aead_encrypt(&k, &nn, &ad, &pt);
        assert_eq!(aead_decrypt(&k, &nn, &ad, &c, &t).unwrap(), pt);
        if !c.is_empty() {
            let mut c2 = c.clone();
            let i = (rng.next() as usize) % c2.len();
            c2[i] ^= 1 << (rng.next() % 8);
            assert!(aead_decrypt(&k, &nn, &ad, &c2, &t).is_err());
        }
    }
    println!("all r221 checks passed");
}
