//! RECIPE 225 (crypto): hash-based signatures: Lamport, Winternitz (WOTS, w=16), and a Merkle signature scheme (XMSS-like).
//! LEVEL: advanced
//! Use in: post-quantum signature design, firmware signing, understanding stateful signatures.   Run: cargo run --release --example hash_based_signatures
#![allow(dead_code)]
//!
//! !!! EDUCATIONAL IMPLEMENTATION. NOT constant-time-audited, NOT for production. Simplified (no per-hash addressing/bitmasks,
//! !!! deterministic demo seeds). Use audited libraries (RustCrypto https://github.com/RustCrypto ,
//! !!! ring https://github.com/briansmith/ring) and never roll your own crypto for real systems.
//!
//! WHAT: signatures whose security rests only on a hash function (SHA-256 here, implemented from scratch and checked against
//! the FIPS 180-4 vectors). No number theory, so no Shor's-algorithm break: this is why they matter for post-quantum.
//!  1. LAMPORT one-time signature: secret = 256 pairs of random 32-byte values; public = their hashes. To sign a digest, reveal,
//!     for each bit, the secret of that bit's value. Signature 8 KiB. ONE-TIME: each signature reveals half the secrets, so
//!     a second signature reveals more and an attacker can assemble signatures on messages you never signed. We show that:
//!     (a) with a reduced 24-bit digest, TWO signatures plus a little grinding forge a chosen message;
//!     (b) with the full 256-bit digest, a handful of signatures (about 10-20) with the same key let us forge an arbitrary chosen message.
//!  2. WINTERNITZ OTS (w=16): sign a base-16 digit d by revealing the chain value H^d(sk) (hash applied d times); the verifier
//!     applies it 15-d more times and compares to the public end of chain. 64 message digits + 3 CHECKSUM digits
//!     (sum of 15-d_i). Without the checksum anyone could advance chains to forge a "larger digit" message; with it, increasing
//!     a message digit forces decreasing a checksum digit, which would need un-hashing. Signature 67*32 = 2144 B (vs 8192 B).
//!  3. MERKLE SIGNATURE SCHEME: 2^h WOTS key pairs; the public key is the Merkle root of their public-key hashes. Signature =
//!     (leaf index, WOTS signature, h sibling hashes = authentication path). Verifier recomputes the WOTS public key from the
//!     signature, folds it up the path by the index bits and compares with the root. The signer is STATEFUL: it must never sign
//!     twice with the same index (state rollback after backup restore or VM snapshot is catastrophic). Hence NIST SP 800-208
//!     limits stateful schemes (LMS/XMSS) to cases where state can be managed, while stateless SPHINCS+ (SLH-DSA, FIPS 205)
//!     trades larger signatures (~8-50 KB) for no state.
//!
//! COMMON USES: XMSS (RFC 8391) and LMS/HSS (RFC 8554) for firmware and boot-image signing where the signer is a controlled HSM;
//! SPHINCS+ / SLH-DSA as the conservative stateless post-quantum signature; Lamport-style schemes as teaching material
//! and in some blockchain research proposals.
//! Read: https://datatracker.ietf.org/doc/html/rfc8391 (XMSS)
//! Read: https://datatracker.ietf.org/doc/html/rfc8554 (LMS)
//! Read: https://sphincs.org/
//! Read: https://csrc.nist.gov/pubs/sp/800/208/final (NIST SP 800-208, stateful hash-based signatures)
use std::sync::OnceLock;

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

fn sha256_cat(parts: &[&[u8]]) -> Hash {
    sha256(&parts.concat())
}

/// Deterministic expander used as a PRF for secret material: H(seed || label || counter). Demo-only (no entropy source).
fn prf(seed: &[u8; 32], label: u8, i: u32) -> Hash {
    sha256_cat(&[seed, &[label], &i.to_be_bytes()])
}

fn test_sha() {
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
    println!("sha256: FIPS 180-4 vectors ok");
}

// ---------------------------------------------------------------- Lamport

/// Lamport over the first `B` bits of the digest (B = 256 is the real scheme; smaller B only to make forgery demos cheap).
struct Lamport<const B: usize> {
    sk: Vec<[Hash; 2]>,
    pk: Vec<[Hash; 2]>,
}

fn digest_bits<const B: usize>(msg: &[u8]) -> Vec<usize> {
    let d = sha256(msg);
    (0..B)
        .map(|i| ((d[i / 8] >> (7 - i % 8)) & 1) as usize)
        .collect()
}

impl<const B: usize> Lamport<B> {
    fn keygen(seed: &[u8; 32]) -> Self {
        let sk: Vec<[Hash; 2]> = (0..B as u32)
            .map(|i| [prf(seed, 0, 2 * i), prf(seed, 0, 2 * i + 1)])
            .collect();
        let pk = sk.iter().map(|p| [sha256(&p[0]), sha256(&p[1])]).collect();
        Lamport { sk, pk }
    }
    fn sign(&self, msg: &[u8]) -> Vec<Hash> {
        digest_bits::<B>(msg)
            .iter()
            .enumerate()
            .map(|(i, &b)| self.sk[i][b])
            .collect()
    }
}
fn lamport_verify<const B: usize>(pk: &[[Hash; 2]], msg: &[u8], sig: &[Hash]) -> bool {
    sig.len() == B
        && digest_bits::<B>(msg)
            .iter()
            .enumerate()
            .all(|(i, &b)| sha256(&sig[i]) == pk[i][b])
}

/// What an attacker knows after seeing signatures: revealed[i][b] = Some(secret) if bit-value b at position i was ever revealed.
struct Leak<const B: usize> {
    known: Vec<[Option<Hash>; 2]>,
}
impl<const B: usize> Leak<B> {
    fn new() -> Self {
        Leak {
            known: vec![[None, None]; B],
        }
    }
    fn absorb(&mut self, msg: &[u8], sig: &[Hash]) {
        for (i, &b) in digest_bits::<B>(msg).iter().enumerate() {
            self.known[i][b] = Some(sig[i]);
        }
    }
    fn can_sign(&self, msg: &[u8]) -> bool {
        digest_bits::<B>(msg)
            .iter()
            .enumerate()
            .all(|(i, &b)| self.known[i][b].is_some())
    }
    fn forge(&self, msg: &[u8]) -> Option<Vec<Hash>> {
        digest_bits::<B>(msg)
            .iter()
            .enumerate()
            .map(|(i, &b)| self.known[i][b])
            .collect()
    }
}

fn test_lamport() {
    let seed = sha256(b"lamport demo seed");
    let k = Lamport::<256>::keygen(&seed);
    let sig = k.sign(b"hello");
    assert!(lamport_verify::<256>(&k.pk, b"hello", &sig));
    assert!(!lamport_verify::<256>(&k.pk, b"hellp", &sig));
    let mut bad = sig.clone();
    bad[7][0] ^= 1;
    assert!(!lamport_verify::<256>(&k.pk, b"hello", &bad));
    println!(
        "lamport-256: sig = {} bytes, pk = {} bytes; verify ok, tamper rejected",
        sig.len() * 32,
        k.pk.len() * 64
    );

    // (a) toy 24-bit digest, two signatures, grind a chosen message: any message whose digest bits are covered.
    let k = Lamport::<24>::keygen(&seed);
    let mut leak = Leak::<24>::new();
    for m in [&b"msg one"[..], b"msg two"] {
        leak.absorb(m, &k.sign(m));
    }
    let target = b"transfer 1000000 to mallory";
    let mut tries = 0u64;
    let forged_msg = loop {
        let cand = [&target[..], format!(" #{tries}").as_bytes()].concat(); // chosen text + a nonce field the attacker controls
        if leak.can_sign(&cand) {
            break cand;
        }
        tries += 1;
    };
    let forged_sig = leak.forge(&forged_msg).unwrap();
    assert!(lamport_verify::<24>(&k.pk, &forged_msg, &forged_sig));
    println!(
        "(a) 24-bit digest, 2 signatures seen: forged {:?} after {tries} tries (expected about (4/3)^24 = 1000)",
        String::from_utf8_lossy(&forged_msg)
    );

    // (b) full 256-bit digest: each extra signature halves the uncovered fraction of each position.
    let k = Lamport::<256>::keygen(&seed);
    let mut leak = Leak::<256>::new();
    let mut count = 0;
    let target = b"chosen message the signer never saw";
    while !leak.can_sign(target) {
        let m = format!("innocent message {count}");
        leak.absorb(m.as_bytes(), &k.sign(m.as_bytes()));
        count += 1;
        assert!(count < 200);
    }
    let forged = leak.forge(target).unwrap();
    assert!(lamport_verify::<256>(&k.pk, target, &forged));
    println!(
        "(b) 256-bit digest: after {count} signatures with ONE key, an arbitrary chosen message is forgeable. One-time means ONE."
    );
}

// ---------------------------------------------------------------- Winternitz (w = 16)

const W: usize = 16;
const LEN1: usize = 64; // 256 bits / 4 bits per digit
const LEN2: usize = 3; // checksum <= 64*15 = 960 < 16^3
const LEN: usize = LEN1 + LEN2;

fn chain(x: &Hash, from: usize, steps: usize) -> Hash {
    let mut v = *x;
    for _ in from..from + steps {
        v = sha256(&v); // real XMSS keys each step with a PRF(pub_seed, address) and a bitmask to avoid multi-target attacks
    }
    v
}

/// Message digits in base 16 followed by the 3 checksum digits.
fn wots_digits(msg: &[u8]) -> [usize; LEN] {
    let d = sha256(msg);
    let mut out = [0usize; LEN];
    for i in 0..32 {
        out[2 * i] = (d[i] >> 4) as usize;
        out[2 * i + 1] = (d[i] & 15) as usize;
    }
    let csum: usize = out[..LEN1].iter().map(|&x| W - 1 - x).sum();
    out[LEN1] = (csum >> 8) & 15;
    out[LEN1 + 1] = (csum >> 4) & 15;
    out[LEN1 + 2] = csum & 15;
    out
}

fn wots_sk(seed: &[u8; 32], idx: u32) -> Vec<Hash> {
    (0..LEN as u32)
        .map(|j| prf(seed, 1, idx * 100 + j))
        .collect()
}
fn wots_pk_from_ends(ends: &[Hash]) -> Hash {
    let flat: Vec<u8> = ends.iter().flatten().copied().collect();
    sha256(&flat)
}
fn wots_pk(seed: &[u8; 32], idx: u32) -> Hash {
    let ends: Vec<Hash> = wots_sk(seed, idx)
        .iter()
        .map(|s| chain(s, 0, W - 1))
        .collect();
    wots_pk_from_ends(&ends)
}
fn wots_sign(seed: &[u8; 32], idx: u32, msg: &[u8]) -> Vec<Hash> {
    let d = wots_digits(msg);
    wots_sk(seed, idx)
        .iter()
        .zip(d)
        .map(|(s, di)| chain(s, 0, di))
        .collect()
}
fn wots_pk_from_sig(msg: &[u8], sig: &[Hash]) -> Hash {
    let d = wots_digits(msg);
    let ends: Vec<Hash> = sig
        .iter()
        .zip(d)
        .map(|(s, di)| chain(s, di, W - 1 - di))
        .collect();
    wots_pk_from_ends(&ends)
}

fn test_wots() {
    let seed = sha256(b"wots demo seed");
    let pk = wots_pk(&seed, 0);
    let sig = wots_sign(&seed, 0, b"firmware v1.2");
    assert_eq!(sig.len(), LEN);
    assert_eq!(wots_pk_from_sig(b"firmware v1.2", &sig), pk);
    assert_ne!(wots_pk_from_sig(b"firmware v1.3", &sig), pk);
    println!(
        "wots w=16: {} chains, sig = {} bytes; verify ok, wrong message rejected",
        LEN,
        LEN * 32
    );

    // Checksum property: if every message digit is >= and one is strictly greater, the checksum is strictly smaller,
    // and a (larger-digit) forgery would need a checksum chain value *earlier* in its chain: impossible without inverting H.
    let a = wots_digits(b"a");
    let mut b = a;
    let i = (0..LEN1).find(|&i| a[i] < 15).unwrap();
    b[i] += 1; // pretend the forger advanced chain i by one step
    let cs = |d: &[usize; LEN]| d[LEN1] * 256 + d[LEN1 + 1] * 16 + d[LEN1 + 2];
    let sum: usize = b[..LEN1].iter().map(|&x| W - 1 - x).sum();
    assert_eq!(sum + 1, cs(&a));
    assert!(
        sum < cs(&a),
        "advancing a message digit must lower the required checksum"
    );
    println!(
        "checksum: advancing a message chain lowers the required checksum value ({} -> {}), which cannot be forged",
        cs(&a),
        sum
    );
}

// ---------------------------------------------------------------- Merkle signature scheme

const H: usize = 4; // 16 one-time keys: tiny so the demo is fast

fn node(l: &Hash, r: &Hash) -> Hash {
    sha256_cat(&[&[1u8], l, r])
}

struct MssSigner {
    seed: [u8; 32],
    levels: Vec<Vec<Hash>>, // levels[0] = leaves, levels[H] = [root]
    next: u32,              // THE STATE. Persist it BEFORE releasing the signature.
}
struct MssSig {
    idx: u32,
    wots: Vec<Hash>,
    path: Vec<Hash>,
}

impl MssSigner {
    fn new(seed: [u8; 32]) -> Self {
        let leaves: Vec<Hash> = (0..1u32 << H).map(|i| wots_pk(&seed, i)).collect();
        let mut levels = vec![leaves];
        for _ in 0..H {
            let prev = levels.last().unwrap();
            levels.push(prev.chunks(2).map(|c| node(&c[0], &c[1])).collect());
        }
        MssSigner {
            seed,
            levels,
            next: 0,
        }
    }
    fn public_key(&self) -> Hash {
        self.levels[H][0]
    }
    fn sign_at(&self, idx: u32, msg: &[u8]) -> MssSig {
        let path = (0..H)
            .map(|l| self.levels[l][((idx >> l) ^ 1) as usize])
            .collect();
        MssSig {
            idx,
            wots: wots_sign(&self.seed, idx, msg),
            path,
        }
    }
    /// Safe API: consumes an index; fails when exhausted.
    fn sign(&mut self, msg: &[u8]) -> Option<MssSig> {
        if self.next >= 1 << H {
            return None;
        }
        let idx = self.next;
        self.next += 1; // advance state first: if we crash after this, we lose a key but never reuse one
        Some(self.sign_at(idx, msg))
    }
}

fn mss_verify(root: &Hash, msg: &[u8], s: &MssSig) -> bool {
    if s.idx >= 1 << H || s.path.len() != H || s.wots.len() != LEN {
        return false;
    }
    let mut h = wots_pk_from_sig(msg, &s.wots);
    for (l, sib) in s.path.iter().enumerate() {
        h = if (s.idx >> l) & 1 == 0 {
            node(&h, sib)
        } else {
            node(sib, &h)
        };
    }
    h == *root
}

fn test_mss() {
    let mut signer = MssSigner::new(sha256(b"mss demo seed"));
    let root = signer.public_key();
    let msgs: Vec<String> = (0..1 << H).map(|i| format!("release {i}")).collect();
    let mut sigs = vec![];
    for m in &msgs {
        let s = signer.sign(m.as_bytes()).unwrap();
        assert!(mss_verify(&root, m.as_bytes(), &s));
        sigs.push(s);
    }
    assert!(
        signer.sign(b"one too many").is_none(),
        "keys exhausted: signer must refuse"
    );
    assert!(!mss_verify(&root, b"release 1", &sigs[0]));
    let mut bad = MssSig {
        idx: sigs[3].idx,
        wots: sigs[3].wots.clone(),
        path: sigs[3].path.clone(),
    };
    bad.path[1][0] ^= 1;
    assert!(!mss_verify(&root, msgs[3].as_bytes(), &bad));
    let wrong_idx = MssSig {
        idx: 4,
        wots: sigs[3].wots.clone(),
        path: sigs[3].path.clone(),
    };
    assert!(!mss_verify(&root, msgs[3].as_bytes(), &wrong_idx));
    println!(
        "merkle sig scheme h={H}: {} signatures verified against one {}-byte root; 17th refused; bad path/index rejected",
        1 << H,
        root.len()
    );

    // State reuse: same index, two messages. Per chain the attacker now knows the EARLIER of the two positions.
    let (m1, m2) = (b"pay bob 1", b"pay bob 2");
    let s1 = signer.sign_at(5, m1).wots;
    let s2 = signer.sign_at(5, m2).wots;
    let (d1, d2) = (wots_digits(m1), wots_digits(m2));
    let mut strictly_deeper = 0;
    for i in 0..LEN {
        let (lo_sig, lo) = if d1[i] <= d2[i] {
            (&s1[i], d1[i])
        } else {
            (&s2[i], d2[i])
        };
        // From the lower-position value the attacker can compute BOTH revealed values and everything beyond.
        assert_eq!(chain(lo_sig, lo, d1[i] - lo), s1[i]);
        assert_eq!(chain(lo_sig, lo, d2[i] - lo), s2[i]);
        if lo < d1[i].max(d2[i]) {
            strictly_deeper += 1;
        }
    }
    println!(
        "index reuse: for {strictly_deeper}/{LEN} chains the attacker now knows a deeper point than one of the signatures; every later message with digits >= those minima is forgeable"
    );
}

fn sig_sizes() {
    fn wots_len(w: usize) -> usize {
        let lg = w.trailing_zeros() as usize;
        let len1 = 256usize.div_ceil(lg);
        let max = len1 * (w - 1);
        let mut len2 = 1;
        while w.pow(len2 as u32) <= max {
            len2 += 1;
        }
        len1 + len2
    }
    assert_eq!(wots_len(16), LEN);
    println!(
        "\n{:<34} {:>12} {:>12}",
        "scheme", "signature B", "public key B"
    );
    println!(
        "{:<34} {:>12} {:>12}",
        "Lamport (SHA-256)",
        256 * 32,
        256 * 64
    );
    for w in [4usize, 16, 256] {
        println!(
            "{:<34} {:>12} {:>12}",
            format!("WOTS w={w} ({} chains)", wots_len(w)),
            wots_len(w) * 32,
            32
        );
    }
    for h in [4usize, 10, 16, 20] {
        println!(
            "{:<34} {:>12} {:>12}",
            format!("this MSS, w=16, h={h} (2^{h} sigs)"),
            4 + LEN * 32 + h * 32,
            32
        );
    }
    println!(
        "{:<34} {:>12} {:>12}",
        "XMSS-SHA2_10_256 (RFC 8391)", 2500, 64
    );
    println!(
        "(for comparison, SPHINCS+/SLH-DSA signatures are ~8-50 KB depending on parameter set; Ed25519 is 64 B but not post-quantum)"
    );
}

fn main() {
    println!(
        "EDUCATIONAL ONLY: not audited, not for production. Use RustCrypto / ring / a vetted XMSS or LMS library.\n"
    );
    test_sha();
    test_lamport();
    test_wots();
    test_mss();
    sig_sizes();
    println!("\nall assertions passed");
}
