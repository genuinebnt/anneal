//! RECIPE 222 (crypto): AES-128 from scratch with ECB, CBC (+padding oracle), CTR and GCM
//! LEVEL: advanced
//! Use in: understanding TLS/disk-encryption/token formats and why modes matter.   Run: cargo run --release --example aes_modes
#![allow(dead_code)]
//!
//! !!! EDUCATIONAL IMPLEMENTATION. NOT constant-time-audited. NOT FOR PRODUCTION. !!!
//! This file uses table lookups indexed by secret data, which leak through cache timing. Use audited libraries
//! (RustCrypto https://github.com/RustCrypto , ring https://github.com/briansmith/ring) and never roll your own
//! crypto for real systems.
//!
//! What: AES-128 (FIPS 197): the S-box is GENERATED (multiplicative inverse in GF(2^8) mod x^8+x^4+x^3+x+1, then
//! an affine map), key expansion, SubBytes / ShiftRows / MixColumns / AddRoundKey and their inverses.
//! Modes (NIST SP 800-38A and 800-38D): ECB, CBC with PKCS#7, CTR, GCM (GHASH = carry-less multiply in GF(2^128)).
//! Why modes matter: a block cipher only encrypts 16 bytes. ECB maps equal plaintext blocks to equal ciphertext
//! blocks (the "ECB penguin": image structure survives). CBC hides patterns but is MALLEABLE (flip a bit in
//! C[i-1] and the same bit flips in P[i]) and has no integrity; if a server reveals whether padding was valid
//! (a "padding oracle"), an attacker decrypts any block byte by byte with ~256 queries per byte, no key needed.
//! This is why AEAD (GCM, ChaCha20-Poly1305) is the default: confidentiality AND integrity, verified before use.
//! GCM caveat: never reuse a (key, nonce) pair; it leaks plaintext XORs and the GHASH key.
//!
//! COMMON USES: AES-GCM in TLS 1.2/1.3 and QUIC, AES-CTR in SSH, AES-CBC in legacy TLS and old file formats
//! (and the origin of many padding-oracle bugs), AES-XTS in disk encryption (a different mode, not shown).
//! Read: https://csrc.nist.gov/pubs/fips/197/final , https://csrc.nist.gov/pubs/sp/800/38/a/final ,
//! https://csrc.nist.gov/pubs/sp/800/38/d/final

type Block = [u8; 16];

fn xtime(b: u8) -> u8 {
    (b << 1) ^ if b & 0x80 != 0 { 0x1b } else { 0 }
}

/// Multiplication in GF(2^8) (shift-and-add).
fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    p
}

/// Build S-box and inverse S-box: s = affine(inverse(x)), with inverse(x) = x^254 and inverse(0) = 0.
fn build_sboxes() -> ([u8; 256], [u8; 256]) {
    let mut sbox = [0u8; 256];
    let mut inv = [0u8; 256];
    for x in 0..256usize {
        let mut r = 1u8; // x^254 by square-and-multiply on the exponent 254 = 0b11111110
        let mut base = x as u8;
        let mut e = 254;
        while e > 0 {
            if e & 1 != 0 {
                r = gmul(r, base);
            }
            base = gmul(base, base);
            e >>= 1;
        }
        let b = if x == 0 { 0 } else { r };
        let s =
            b ^ b.rotate_left(1) ^ b.rotate_left(2) ^ b.rotate_left(3) ^ b.rotate_left(4) ^ 0x63;
        sbox[x] = s;
        inv[s as usize] = x as u8;
    }
    (sbox, inv)
}

struct Aes128 {
    rk: [Block; 11],
    sbox: [u8; 256],
    inv: [u8; 256],
}

impl Aes128 {
    fn new(key: &[u8; 16]) -> Self {
        let (sbox, inv) = build_sboxes();
        let mut w = [[0u8; 4]; 44];
        for i in 0..4 {
            w[i].copy_from_slice(&key[4 * i..4 * i + 4]);
        }
        let mut rcon = 1u8;
        for i in 4..44 {
            let mut t = w[i - 1];
            if i % 4 == 0 {
                t = [
                    sbox[t[1] as usize] ^ rcon,
                    sbox[t[2] as usize],
                    sbox[t[3] as usize],
                    sbox[t[0] as usize],
                ];
                rcon = xtime(rcon);
            }
            for j in 0..4 {
                w[i][j] = w[i - 4][j] ^ t[j];
            }
        }
        let mut rk = [[0u8; 16]; 11];
        for r in 0..11 {
            for c in 0..4 {
                rk[r][4 * c..4 * c + 4].copy_from_slice(&w[4 * r + c]);
            }
        }
        Aes128 { rk, sbox, inv }
    }

    // State layout: s[4*col + row], identical to input byte order.
    fn add_round_key(s: &mut Block, k: &Block) {
        for i in 0..16 {
            s[i] ^= k[i];
        }
    }
    fn shift_rows(s: &mut Block) {
        let o = *s;
        for c in 0..4 {
            for r in 0..4 {
                s[4 * c + r] = o[4 * ((c + r) % 4) + r];
            }
        }
    }
    fn inv_shift_rows(s: &mut Block) {
        let o = *s;
        for c in 0..4 {
            for r in 0..4 {
                s[4 * ((c + r) % 4) + r] = o[4 * c + r];
            }
        }
    }
    fn mix_columns(s: &mut Block, inverse: bool) {
        let m: [u8; 4] = if inverse {
            [14, 11, 13, 9]
        } else {
            [2, 3, 1, 1]
        };
        for c in 0..4 {
            let col = [s[4 * c], s[4 * c + 1], s[4 * c + 2], s[4 * c + 3]];
            for r in 0..4 {
                // Row r of the circulant matrix is the first row rotated right by r.
                s[4 * c + r] = (0..4).fold(0, |acc, k| acc ^ gmul(m[(k + 4 - r) % 4], col[k]));
            }
        }
    }

    fn encrypt_block(&self, input: &Block) -> Block {
        let mut s = *input;
        Self::add_round_key(&mut s, &self.rk[0]);
        for round in 1..=10 {
            for b in s.iter_mut() {
                *b = self.sbox[*b as usize];
            }
            Self::shift_rows(&mut s);
            if round != 10 {
                Self::mix_columns(&mut s, false);
            }
            Self::add_round_key(&mut s, &self.rk[round]);
        }
        s
    }

    fn decrypt_block(&self, input: &Block) -> Block {
        let mut s = *input;
        Self::add_round_key(&mut s, &self.rk[10]);
        for round in (0..10).rev() {
            Self::inv_shift_rows(&mut s);
            for b in s.iter_mut() {
                *b = self.inv[*b as usize];
            }
            Self::add_round_key(&mut s, &self.rk[round]);
            if round != 0 {
                Self::mix_columns(&mut s, true);
            }
        }
        s
    }
}

fn xor16(a: &Block, b: &Block) -> Block {
    core::array::from_fn(|i| a[i] ^ b[i])
}

// ------------------------------------------------------------------ ECB / CBC

fn ecb_encrypt(aes: &Aes128, data: &[u8]) -> Vec<u8> {
    assert!(data.len() % 16 == 0);
    data.chunks(16)
        .flat_map(|c| aes.encrypt_block(c.try_into().unwrap()))
        .collect()
}

fn cbc_encrypt_raw(aes: &Aes128, iv: &Block, data: &[u8]) -> Vec<u8> {
    assert!(data.len() % 16 == 0);
    let mut prev = *iv;
    let mut out = Vec::new();
    for c in data.chunks(16) {
        prev = aes.encrypt_block(&xor16(&prev, c.try_into().unwrap()));
        out.extend_from_slice(&prev);
    }
    out
}

fn cbc_decrypt_raw(aes: &Aes128, iv: &Block, data: &[u8]) -> Vec<u8> {
    assert!(data.len() % 16 == 0);
    let mut prev = *iv;
    let mut out = Vec::new();
    for c in data.chunks(16) {
        let c: Block = c.try_into().unwrap();
        out.extend_from_slice(&xor16(&prev, &aes.decrypt_block(&c)));
        prev = c;
    }
    out
}

fn pkcs7_pad(data: &[u8]) -> Vec<u8> {
    let n = 16 - data.len() % 16;
    let mut v = data.to_vec();
    v.extend(std::iter::repeat(n as u8).take(n));
    v
}

fn pkcs7_unpad(data: &[u8]) -> Option<&[u8]> {
    let n = *data.last()? as usize;
    if n == 0 || n > 16 || n > data.len() || data[data.len() - n..].iter().any(|&b| b as usize != n)
    {
        return None;
    }
    Some(&data[..data.len() - n])
}

fn cbc_encrypt(aes: &Aes128, iv: &Block, pt: &[u8]) -> Vec<u8> {
    cbc_encrypt_raw(aes, iv, &pkcs7_pad(pt))
}

fn cbc_decrypt(aes: &Aes128, iv: &Block, ct: &[u8]) -> Option<Vec<u8>> {
    if ct.is_empty() || ct.len() % 16 != 0 {
        return None;
    }
    pkcs7_unpad(&cbc_decrypt_raw(aes, iv, ct)).map(|p| p.to_vec())
}

/// The vulnerable server behaviour: tells the caller ONLY whether the padding was valid.
fn padding_oracle(aes: &Aes128, iv: &Block, ct: &[u8]) -> bool {
    cbc_decrypt(aes, iv, ct).is_some()
}

/// Recover D_K(target) (the "intermediate" block) and hence the plaintext of `target`, given only the oracle
/// and the previous ciphertext block (or IV). Never touches the key.
fn padding_oracle_attack(
    oracle: impl Fn(&Block, &Block) -> bool,
    prev: &Block,
    target: &Block,
) -> (Block, usize) {
    let mut inter = [0u8; 16];
    let mut queries = 0;
    for n in 1..=16usize {
        let pos = 16 - n;
        // Forge a previous block so bytes already known decrypt to padding value n.
        let mut forged = [0u8; 16];
        for j in pos + 1..16 {
            forged[j] = inter[j] ^ n as u8;
        }
        let mut found = false;
        for g in 0..=255u8 {
            forged[pos] = g;
            queries += 1;
            if !oracle(&forged, target) {
                continue;
            }
            if n == 1 && pos > 0 {
                // Rule out a false positive such as ...02 02: change the byte before and re-ask.
                let mut f2 = forged;
                f2[pos - 1] ^= 0xff;
                queries += 1;
                if !oracle(&f2, target) {
                    continue;
                }
            }
            inter[pos] = g ^ n as u8;
            found = true;
            break;
        }
        assert!(found, "oracle gave no valid padding");
    }
    (xor16(&inter, prev), queries)
}

// ------------------------------------------------------------------ CTR / GCM

fn ctr_xor(aes: &Aes128, counter0: &Block, data: &[u8], inc_bytes: usize) -> Vec<u8> {
    let mut ctr = *counter0;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let ks = aes.encrypt_block(&ctr);
        out.extend(chunk.iter().zip(ks).map(|(d, k)| d ^ k));
        // big-endian increment of the last `inc_bytes` bytes
        for i in (16 - inc_bytes..16).rev() {
            ctr[i] = ctr[i].wrapping_add(1);
            if ctr[i] != 0 {
                break;
            }
        }
    }
    out
}

/// GF(2^128) multiply with GCM's bit-reflected convention (SP 800-38D, Algorithm 1).
fn gf128_mul(x: u128, y: u128) -> u128 {
    let mut z = 0u128;
    let mut v = y;
    for i in 0..128 {
        if (x >> (127 - i)) & 1 == 1 {
            z ^= v; // carry-less "add"
        }
        let lsb = v & 1;
        v >>= 1;
        if lsb == 1 {
            v ^= 0xe1u128 << 120; // reduction by x^128 + x^7 + x^2 + x + 1
        }
    }
    z
}

fn ghash(h: u128, aad: &[u8], ct: &[u8]) -> u128 {
    let mut y = 0u128;
    for data in [aad, ct] {
        for chunk in data.chunks(16) {
            let mut b = [0u8; 16];
            b[..chunk.len()].copy_from_slice(chunk);
            y = gf128_mul(y ^ u128::from_be_bytes(b), h);
        }
    }
    let lens = ((aad.len() as u128 * 8) << 64) | (ct.len() as u128 * 8);
    gf128_mul(y ^ lens, h)
}

fn gcm_seal(aes: &Aes128, iv: &[u8; 12], aad: &[u8], pt: &[u8]) -> (Vec<u8>, Block) {
    let h = u128::from_be_bytes(aes.encrypt_block(&[0; 16]));
    let mut j0 = [0u8; 16];
    j0[..12].copy_from_slice(iv);
    j0[15] = 1;
    let mut c1 = j0;
    c1[15] = 2;
    let ct = ctr_xor(aes, &c1, pt, 4);
    let s = ghash(h, aad, &ct) ^ u128::from_be_bytes(aes.encrypt_block(&j0));
    (ct, s.to_be_bytes())
}

fn gcm_open(aes: &Aes128, iv: &[u8; 12], aad: &[u8], ct: &[u8], tag: &Block) -> Option<Vec<u8>> {
    // Recompute the tag over the ciphertext first; only decrypt if it matches.
    let h = u128::from_be_bytes(aes.encrypt_block(&[0; 16]));
    let mut j0 = [0u8; 16];
    j0[..12].copy_from_slice(iv);
    j0[15] = 1;
    let expect = (ghash(h, aad, ct) ^ u128::from_be_bytes(aes.encrypt_block(&j0))).to_be_bytes();
    if expect.iter().zip(tag).fold(0u8, |a, (x, y)| a | (x ^ y)) != 0 {
        return None;
    }
    let mut c1 = j0;
    c1[15] = 2;
    Some(ctr_xor(aes, &c1, ct, 4))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

fn blk(s: &str) -> Block {
    unhex(s).try_into().unwrap()
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

fn main() {
    // S-box sanity (generated, not typed in): FIPS-197 Figure 7 corners.
    let (sb, _) = build_sboxes();
    assert_eq!(
        (sb[0x00], sb[0x01], sb[0x53], sb[0xff]),
        (0x63, 0x7c, 0xed, 0x16)
    );

    // FIPS-197 Appendix B and C.1
    let aes = Aes128::new(&blk("2b7e151628aed2a6abf7158809cf4f3c"));
    let ct = aes.encrypt_block(&blk("3243f6a8885a308d313198a2e0370734"));
    assert_eq!(hex(&ct), "3925841d02dc09fbdc118597196a0b32");
    assert_eq!(
        hex(&aes.decrypt_block(&ct)),
        "3243f6a8885a308d313198a2e0370734"
    );
    let aes_c1 = Aes128::new(&blk("000102030405060708090a0b0c0d0e0f"));
    let ct = aes_c1.encrypt_block(&blk("00112233445566778899aabbccddeeff"));
    assert_eq!(hex(&ct), "69c4e0d86a7b0430d8cdb78070b4c55a");
    assert_eq!(
        hex(&aes_c1.decrypt_block(&ct)),
        "00112233445566778899aabbccddeeff"
    );
    println!("AES-128 FIPS-197 B and C.1 OK: {}", hex(&ct));

    // SP 800-38A shared plaintext
    let key = Aes128::new(&blk("2b7e151628aed2a6abf7158809cf4f3c"));
    let pt = unhex(
        "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
    );

    // CBC F.2.1
    let iv = blk("000102030405060708090a0b0c0d0e0f");
    let cbc_ct = cbc_encrypt_raw(&key, &iv, &pt);
    assert_eq!(
        hex(&cbc_ct),
        "7649abac8119b246cee98e9b12e9197d5086cb9b507219ee95db113a917678b273bed6b8e3c1743b7116e69e222295163ff1caa1681fac09120eca307586e1a7"
    );
    assert_eq!(cbc_decrypt_raw(&key, &iv, &cbc_ct), pt);
    println!("CBC SP 800-38A F.2.1 OK");

    // CTR F.5.1
    let ctr0 = blk("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
    let ctr_ct = ctr_xor(&key, &ctr0, &pt, 16);
    assert_eq!(
        hex(&ctr_ct),
        "874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff5ae4df3edbd5d35e5b4f09020db03eab1e031dda2fbe03d1792170a0f3009cee"
    );
    assert_eq!(ctr_xor(&key, &ctr0, &ctr_ct, 16), pt);
    println!("CTR SP 800-38A F.5.1 OK");

    // ECB penguin: repeated plaintext blocks are visible in the ciphertext.
    let repeated = [b"YELLOW SUBMARINE".as_slice(); 4].concat();
    let e = ecb_encrypt(&key, &repeated);
    assert_eq!(e[..16], e[16..32]);
    assert_eq!(e[16..32], e[32..48]);
    let c = cbc_encrypt_raw(&key, &iv, &repeated);
    assert_ne!(c[..16], c[16..32]);
    println!("ECB: 4 identical plaintext blocks -> 4 identical ciphertext blocks (CBC hides this)");

    // PKCS#7 validation
    assert_eq!(pkcs7_unpad(&pkcs7_pad(b"")).unwrap(), b"");
    assert_eq!(pkcs7_pad(&[0u8; 16]).len(), 32);
    assert!(pkcs7_unpad(&[1, 2, 3, 0]).is_none());
    assert!(pkcs7_unpad(&[2; 15].iter().chain(&[3]).copied().collect::<Vec<u8>>()).is_none());
    assert!(pkcs7_unpad(&[17; 16]).is_none());

    // CBC malleability: flipping bits in C[0] flips the same bits in P[1] (and garbles P[0]).
    let secret = b"pay=0000100.00 USD; account=7777"; // 32 bytes
    let ct2 = cbc_encrypt_raw(&key, &iv, secret);
    let mut tampered = ct2.clone();
    tampered[4] ^= b'0' ^ b'9'; // flips the same bits in P[1][4]
    let dec = cbc_decrypt_raw(&key, &iv, &tampered);
    assert_eq!(dec[16 + 4], secret[16 + 4] ^ b'0' ^ b'9');
    println!(
        "CBC is malleable: a bit-flip in C0 predictably edits P1 (no integrity without a MAC)"
    );

    // Padding oracle attack on ONE block, using only the oracle (no key access in the attacker).
    let iv2: Block = blk("101112131415161718191a1b1c1d1e1f");
    let msg = b"top secret data!"; // 16 bytes -> padded to 2 blocks (second is all 0x10)
    let c = cbc_encrypt(&key, &iv2, msg);
    assert_eq!(c.len(), 32);
    let target: Block = c[..16].try_into().unwrap();
    // The oracle treats the forged block as the IV of a one-block message.
    let oracle = |prev: &Block, tgt: &Block| padding_oracle(&key, prev, tgt);
    let (recovered, queries) = padding_oracle_attack(oracle, &iv2, &target);
    assert_eq!(&recovered, msg);
    println!(
        "PADDING ORACLE: recovered {:?} in {} queries, never touching the key",
        String::from_utf8_lossy(&recovered),
        queries
    );

    // GCM: McGrew-Viega / SP 800-38D test cases 1, 2 and 3
    let zk = Aes128::new(&[0; 16]);
    let (c, t) = gcm_seal(&zk, &[0; 12], b"", b"");
    assert!(c.is_empty());
    assert_eq!(hex(&t), "58e2fccefa7e3061367f1d57a4e7455a");
    let (c, t) = gcm_seal(&zk, &[0; 12], b"", &[0; 16]);
    assert_eq!(hex(&c), "0388dace60b6a392f328c2b971b2fe78");
    assert_eq!(hex(&t), "ab6e47d42cec13bdf53a67b21257bddf");
    let gk = Aes128::new(&blk("feffe9928665731c6d6a8f9467308308"));
    let giv: [u8; 12] = unhex("cafebabefacedbaddecaf888").try_into().unwrap();
    let gpt = unhex(
        "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255",
    );
    let (gc, gt) = gcm_seal(&gk, &giv, b"", &gpt);
    assert_eq!(
        hex(&gc),
        "42831ec2217774244b7221b784d0d49ce3aa212f2c02a4e035c17e2329aca12e21d514b25466931c7d8f6a5aac84aa051ba30b396a0aac973d58e091473f5985"
    );
    assert_eq!(hex(&gt), "4d5c2af327cd64a62cf35abd2ba6fab4");
    assert_eq!(gcm_open(&gk, &giv, b"", &gc, &gt).unwrap(), gpt);
    let mut bad = gc.clone();
    bad[0] ^= 1;
    assert!(gcm_open(&gk, &giv, b"", &bad, &gt).is_none());
    assert!(gcm_open(&gk, &giv, b"hdr", &gc, &gt).is_none());
    println!(
        "GCM test cases 1-3 OK; tampered ciphertext/AAD rejected, tag = {}",
        hex(&gt)
    );

    // Seeded random round trips for every mode.
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    for n in 0..100usize {
        let k: Block = rng.bytes(16).try_into().unwrap();
        let a = Aes128::new(&k);
        let iv: Block = rng.bytes(16).try_into().unwrap();
        let p = rng.bytes(n * 5);
        assert_eq!(cbc_decrypt(&a, &iv, &cbc_encrypt(&a, &iv, &p)).unwrap(), p);
        assert_eq!(ctr_xor(&a, &iv, &ctr_xor(&a, &iv, &p, 16), 16), p);
        let nonce: [u8; 12] = rng.bytes(12).try_into().unwrap();
        let ad = rng.bytes(n % 20);
        let (c, t) = gcm_seal(&a, &nonce, &ad, &p);
        assert_eq!(gcm_open(&a, &nonce, &ad, &c, &t).unwrap(), p);
        let bk: Block = rng.bytes(16).try_into().unwrap();
        assert_eq!(a.decrypt_block(&a.encrypt_block(&bk)), bk);
    }
    println!("all r222 checks passed");
}
