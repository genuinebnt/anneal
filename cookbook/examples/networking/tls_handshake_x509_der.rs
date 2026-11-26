//! RECIPE 433 (network): ASN.1 DER, an X.509 certificate parser and chain validator, and a model of the TLS 1.3 handshake with its HKDF key schedule
//! LEVEL: advanced
//! Use in: TLS libraries (rustls/webpki), certificate tooling (openssl x509, step-ca), mTLS and service identity, protocol debugging.   Run: cargo run --release --example tls_handshake_x509_der
#![allow(dead_code)]
//!
//! WHAT: three layers, bottom up.
//!  (a) ASN.1 DER (ITU-T X.690): every value is TAG | LENGTH | VALUE. DER is the canonical subset of BER:
//!      definite lengths only, the SHORTEST length encoding, minimal INTEGERs, BOOLEAN TRUE = 0xFF, BIT STRING
//!      padding bits zero. Canonical matters because signatures are computed over the exact bytes: if a value could be
//!      encoded two ways, an attacker could re-encode without breaking the signature (a classic source of
//!      certificate-parsing bugs). The decoder here is STRICT (rejects non-minimal lengths, indefinite lengths,
//!      non-minimal integers, bad OIDs, depth over 12, trailing bytes) and an encoder is its inverse: 2000 random
//!      structures round-trip, re-encoding is byte-identical, and 30,000 random/mutated inputs never panic it.
//!  (b) X.509 (RFC 5280): a certificate is SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }.
//!      We ASSEMBLE one by hand with the encoder (version, serial, issuer/subject RDNs, validity, SubjectPublicKeyInfo,
//!      extensions basicConstraints / keyUsage / subjectAltName), parse it back, match host names (wildcard only as
//!      the whole leftmost label and matching exactly one label), check validity against an INJECTED clock, and walk a
//!      chain (issuer == subject links, signatures, CA flag, keyCertSign, pathLenConstraint, trust anchor).
//!      SIGNATURES ARE A TOY: real X.509 uses RSA or ECDSA/EdDSA (see examples/cryptography/modular_arithmetic_dh_rsa.rs
//!      for RSA). Here "signature = HMAC-SHA256(issuer public bytes, tbs)", which anyone can forge: it only
//!      demonstrates WHERE verification sits in the algorithm and that any bit flip in the TBS breaks it.
//!  (c) TLS 1.3 (RFC 8446): the message flow ClientHello -> ServerHello -> {EncryptedExtensions, Certificate,
//!      CertificateVerify, Finished} -> client Finished, with the real KEY SCHEDULE: HKDF-Extract/Expand-Label
//!      chains early -> handshake -> master secret, each traffic secret bound to a TRANSCRIPT HASH of all messages
//!      so far. Finished = HMAC(finished_key, transcript hash), so any changed bit anywhere in the handshake makes the
//!      peers disagree and the handshake fails. Modelled: message framing/parsing, key schedule, certificate
//!      verification inside the handshake. Replaced by toys: the key exchange (finite-field DH mod 2^61-1 instead of
//!      X25519/ECDHE), the signature (see above), and record protection (real TLS encrypts every message after
//!      ServerHello with AEAD under the traffic keys, see chacha20_poly1305.rs; omitted). Not modelled: HelloRetry,
//!      PSK/0-RTT, tickets, alerts, extensions beyond the few needed.
//!
//! COMMON USES: every HTTPS connection; mTLS between services; certificate pinning and expiry monitoring;
//! reading `openssl x509 -text` output (this is what it decodes); building private CAs; understanding why
//! "certificate verify failed" happens (name mismatch vs expiry vs unknown issuer vs path length).
//! Read: RFC 8446 (TLS 1.3) https://www.rfc-editor.org/rfc/rfc8446 ; RFC 5280 (X.509 profile)
//! https://www.rfc-editor.org/rfc/rfc5280 ; ITU-T X.690 (BER/DER) https://www.itu.int/rec/T-REC-X.690 ;
//! RFC 8448 (example TLS 1.3 traces) https://www.rfc-editor.org/rfc/rfc8448 ; RFC 5869 (HKDF) https://www.rfc-editor.org/rfc/rfc5869

// ---- SHA-256 / HMAC / HKDF, copied from examples/cryptography/hmac_hkdf_pbkdf2.rs (see that recipe for the walk-through)
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

// ======================================================================================================
// (a) ASN.1 DER
// ======================================================================================================
#[derive(Debug, Clone, PartialEq)]
enum Der {
    Bool(bool),
    Int(Vec<u8>), // minimal two's complement, big-endian
    BitString { unused: u8, bytes: Vec<u8> },
    Octets(Vec<u8>),
    Null,
    Oid(Vec<u32>),
    Utf8(String),
    Printable(String),
    Ia5(String),
    UtcTime(String),
    GenTime(String),
    Seq(Vec<Der>),
    Set(Vec<Der>),
    Ctx(u8, Vec<Der>),    // [n] constructed (EXPLICIT tagging)
    CtxPrim(u8, Vec<u8>), // [n] primitive (IMPLICIT tagging)
}

#[derive(Debug, Clone, PartialEq)]
enum DerError {
    Truncated,
    BadLength(&'static str),
    BadTag(&'static str),
    Invalid(&'static str),
    Trailing,
    TooDeep,
}
const MAX_DEPTH: usize = 12;

impl Der {
    fn uint(v: u64) -> Der {
        Der::uint_bytes(&v.to_be_bytes())
    }
    /// Unsigned big-endian bytes -> canonical INTEGER (strip leading zeros, add one if the top bit is set).
    fn uint_bytes(b: &[u8]) -> Der {
        let s = match b.iter().position(|&x| x != 0) {
            Some(i) => &b[i..],
            None => &[],
        };
        let mut v = Vec::new();
        if s.is_empty() || s[0] & 0x80 != 0 {
            v.push(0);
        }
        v.extend(s);
        Der::Int(v)
    }
}

fn enc_len(n: usize, out: &mut Vec<u8>) {
    if n < 128 {
        out.push(n as u8);
    } else {
        let b = n.to_be_bytes();
        let s = b.iter().position(|&x| x != 0).unwrap();
        out.push(0x80 | (b.len() - s) as u8);
        out.extend(&b[s..]);
    }
}
fn tlv(id: u8, content: &[u8]) -> Vec<u8> {
    let mut v = vec![id];
    enc_len(content.len(), &mut v);
    v.extend(content);
    v
}
fn base128(mut n: u64, out: &mut Vec<u8>) {
    let mut tmp = vec![(n & 0x7f) as u8];
    n >>= 7;
    while n > 0 {
        tmp.push(0x80 | (n & 0x7f) as u8);
        n >>= 7;
    }
    tmp.reverse();
    out.extend(tmp);
}

fn encode(d: &Der) -> Vec<u8> {
    let list = |v: &[Der]| v.iter().flat_map(encode).collect::<Vec<u8>>();
    match d {
        Der::Bool(b) => tlv(0x01, &[if *b { 0xff } else { 0 }]),
        Der::Int(v) => tlv(0x02, v),
        Der::BitString { unused, bytes } => tlv(0x03, &[&[*unused][..], bytes].concat()),
        Der::Octets(v) => tlv(0x04, v),
        Der::Null => tlv(0x05, &[]),
        Der::Oid(a) => {
            let mut c = Vec::new();
            base128(a[0] as u64 * 40 + a[1] as u64, &mut c);
            for &x in &a[2..] {
                base128(x as u64, &mut c);
            }
            tlv(0x06, &c)
        }
        Der::Utf8(s) => tlv(0x0c, s.as_bytes()),
        Der::Printable(s) => tlv(0x13, s.as_bytes()),
        Der::Ia5(s) => tlv(0x16, s.as_bytes()),
        Der::UtcTime(s) => tlv(0x17, s.as_bytes()),
        Der::GenTime(s) => tlv(0x18, s.as_bytes()),
        Der::Seq(v) => tlv(0x30, &list(v)),
        Der::Set(v) => tlv(0x31, &list(v)),
        Der::Ctx(n, v) => tlv(0xa0 | n, &list(v)),
        Der::CtxPrim(n, v) => tlv(0x80 | n, v),
    }
}

fn printable_ok(s: &[u8]) -> bool {
    s.iter()
        .all(|&c| c.is_ascii_alphanumeric() || b" '()+,-./:=?".contains(&c))
}

/// Decode one TLV from the front of `input`; returns the value and the remaining bytes.
fn decode_one(input: &[u8], depth: usize) -> Result<(Der, &[u8]), DerError> {
    if depth > MAX_DEPTH {
        return Err(DerError::TooDeep);
    }
    let (&id, rest) = input.split_first().ok_or(DerError::Truncated)?;
    if id & 0x1f == 0x1f {
        return Err(DerError::BadTag("high-tag-number form not supported"));
    }
    let (&l0, rest) = rest.split_first().ok_or(DerError::Truncated)?;
    let (len, rest) = if l0 < 0x80 {
        (l0 as usize, rest)
    } else if l0 == 0x80 {
        return Err(DerError::BadLength("indefinite length is BER, not DER"));
    } else if l0 == 0xff {
        return Err(DerError::BadLength("reserved length octet"));
    } else {
        let n = (l0 & 0x7f) as usize;
        if n > 4 {
            return Err(DerError::BadLength("length too large"));
        }
        if rest.len() < n {
            return Err(DerError::Truncated);
        }
        let v = rest[..n].iter().fold(0usize, |a, &b| a << 8 | b as usize);
        if rest[0] == 0 || v < 128 {
            return Err(DerError::BadLength("non-minimal length encoding"));
        }
        (v, &rest[n..])
    };
    if rest.len() < len {
        return Err(DerError::Truncated);
    }
    let (c, rest) = rest.split_at(len);
    let constructed = id & 0x20 != 0;
    let tag = id & 0x1f;
    let children = |c: &[u8]| -> Result<Vec<Der>, DerError> {
        let mut out = Vec::new();
        let mut r = c;
        while !r.is_empty() {
            let (d, n) = decode_one(r, depth + 1)?;
            out.push(d);
            r = n;
        }
        Ok(out)
    };
    let prim = |ok: bool| {
        if ok {
            Ok(())
        } else {
            Err(DerError::BadTag(
                "wrong primitive/constructed form for this type",
            ))
        }
    };
    let v = match id >> 6 {
        0 => match tag {
            0x01 => {
                prim(!constructed)?;
                match c {
                    [0] => Der::Bool(false),
                    [0xff] => Der::Bool(true),
                    _ => return Err(DerError::Invalid("BOOLEAN must be 00 or FF")),
                }
            }
            0x02 => {
                prim(!constructed)?;
                if c.is_empty()
                    || (c.len() > 1
                        && ((c[0] == 0 && c[1] & 0x80 == 0) || (c[0] == 0xff && c[1] & 0x80 != 0)))
                {
                    return Err(DerError::Invalid("INTEGER empty or not minimal"));
                }
                Der::Int(c.to_vec())
            }
            0x03 => {
                prim(!constructed)?;
                let (&u, b) = c
                    .split_first()
                    .ok_or(DerError::Invalid("empty BIT STRING"))?;
                if u > 7
                    || (b.is_empty() && u != 0)
                    || b.last()
                        .is_some_and(|&l| u > 0 && l & ((1u8 << u) - 1) != 0)
                {
                    return Err(DerError::Invalid("bad BIT STRING padding"));
                }
                Der::BitString {
                    unused: u,
                    bytes: b.to_vec(),
                }
            }
            0x04 => {
                prim(!constructed)?;
                Der::Octets(c.to_vec())
            }
            0x05 => {
                prim(!constructed)?;
                if !c.is_empty() {
                    return Err(DerError::Invalid("NULL with content"));
                }
                Der::Null
            }
            0x06 => {
                prim(!constructed)?;
                let mut subs: Vec<u64> = Vec::new();
                let mut cur: u64 = 0;
                let mut start = true;
                for &b in c {
                    if start && b == 0x80 {
                        return Err(DerError::Invalid("OID subidentifier not minimal"));
                    }
                    start = false;
                    cur = cur
                        .checked_mul(128)
                        .ok_or(DerError::Invalid("OID arc overflow"))?
                        | (b & 0x7f) as u64;
                    if cur > u32::MAX as u64 + 80 {
                        return Err(DerError::Invalid("OID arc too large"));
                    }
                    if b & 0x80 == 0 {
                        subs.push(cur);
                        cur = 0;
                        start = true;
                    }
                }
                if !start || subs.is_empty() {
                    return Err(DerError::Invalid("OID truncated or empty"));
                }
                let f = subs[0];
                let (a, b) = if f < 40 {
                    (0, f)
                } else if f < 80 {
                    (1, f - 40)
                } else {
                    (2, f - 80)
                };
                let mut arcs = vec![
                    a as u32,
                    u32::try_from(b).map_err(|_| DerError::Invalid("OID arc too large"))?,
                ];
                for &s in &subs[1..] {
                    arcs.push(
                        u32::try_from(s).map_err(|_| DerError::Invalid("OID arc too large"))?,
                    );
                }
                Der::Oid(arcs)
            }
            0x0c | 0x13 | 0x16 | 0x17 | 0x18 => {
                prim(!constructed)?;
                let s =
                    std::str::from_utf8(c).map_err(|_| DerError::Invalid("string is not UTF-8"))?;
                match tag {
                    0x13 if !printable_ok(c) => {
                        return Err(DerError::Invalid("PrintableString charset"));
                    }
                    0x16 if !c.is_ascii() => {
                        return Err(DerError::Invalid("IA5String must be ASCII"));
                    }
                    0x17 if !(c.len() == 13
                        && c[..12].iter().all(u8::is_ascii_digit)
                        && c[12] == b'Z') =>
                    {
                        return Err(DerError::Invalid("UTCTime must be YYMMDDHHMMSSZ"));
                    }
                    0x18 if !(c.len() == 15
                        && c[..14].iter().all(u8::is_ascii_digit)
                        && c[14] == b'Z') =>
                    {
                        return Err(DerError::Invalid("GeneralizedTime must be YYYYMMDDHHMMSSZ"));
                    }
                    _ => {}
                }
                match tag {
                    0x0c => Der::Utf8(s.into()),
                    0x13 => Der::Printable(s.into()),
                    0x16 => Der::Ia5(s.into()),
                    0x17 => Der::UtcTime(s.into()),
                    _ => Der::GenTime(s.into()),
                }
            }
            0x10 => {
                prim(constructed)?;
                Der::Seq(children(c)?)
            }
            0x11 => {
                prim(constructed)?;
                Der::Set(children(c)?)
            }
            _ => return Err(DerError::BadTag("unsupported universal tag")),
        },
        2 if constructed => Der::Ctx(tag, children(c)?),
        2 => Der::CtxPrim(tag, c.to_vec()),
        _ => return Err(DerError::BadTag("application/private class not supported")),
    };
    Ok((v, rest))
}

fn decode(input: &[u8]) -> Result<Der, DerError> {
    let (d, rest) = decode_one(input, 0)?;
    if rest.is_empty() {
        Ok(d)
    } else {
        Err(DerError::Trailing)
    }
}

fn oid_str(a: &[u32]) -> String {
    a.iter().map(u32::to_string).collect::<Vec<_>>().join(".")
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

fn digits(r: &mut Rng, n: usize) -> String {
    (0..n).map(|_| (b'0' + r.below(10) as u8) as char).collect()
}

fn gen_der(r: &mut Rng, depth: usize) -> Der {
    let kinds = if depth >= 4 { 12 } else { 16 };
    match r.below(kinds) {
        0 => Der::Bool(r.below(2) == 0),
        1 => {
            let n = 1 + r.below(20);
            let mut b = r.bytes(n);
            while b.len() > 1
                && ((b[0] == 0 && b[1] & 0x80 == 0) || (b[0] == 0xff && b[1] & 0x80 != 0))
            {
                b.remove(0); // canonicalise: drop redundant leading 00/FF bytes
            }
            Der::Int(b)
        }
        2 => {
            let n = r.below(10);
            let mut bytes = r.bytes(n);
            let unused = if bytes.is_empty() {
                0
            } else {
                r.below(8) as u8
            };
            if let Some(l) = bytes.last_mut() {
                *l &= !((1u16 << unused) - 1) as u8;
            }
            Der::BitString { unused, bytes }
        }
        3 => {
            let n = r.below(300); // lengths past 127 and 255 exercise long-form lengths
            Der::Octets(r.bytes(n))
        }
        4 => Der::Null,
        5 => {
            let first = r.below(3) as u32;
            let second = if first < 2 {
                r.below(40) as u32
            } else {
                r.below(1_000_000) as u32
            };
            let mut a = vec![first, second];
            for _ in 0..r.below(8) {
                a.push(if r.below(3) == 0 {
                    r.next() as u32
                } else {
                    r.below(300) as u32
                });
            }
            Der::Oid(a)
        }
        6 => Der::Utf8(
            [
                "",
                "h\u{e9}llo",
                "\u{65e5}\u{672c}",
                "plain",
                "crab \u{1F980}",
            ][r.below(5)]
            .repeat(1 + r.below(3)),
        ),
        7 => {
            let n = r.below(30);
            Der::Printable(
                (0..n)
                    .map(|_| b"ABCxyz019 '()+,-./:=?"[r.below(21)] as char)
                    .collect(),
            )
        }
        8 => {
            let n = r.below(40);
            Der::Ia5((0..n).map(|_| (32 + r.below(95)) as u8 as char).collect())
        }
        9 => Der::UtcTime(format!("{}Z", digits(r, 12))),
        10 => Der::GenTime(format!("{}Z", digits(r, 14))),
        11 => {
            let n = r.below(40);
            Der::CtxPrim(r.below(31) as u8, r.bytes(n))
        }
        12 | 13 => {
            let n = r.below(5);
            Der::Seq((0..n).map(|_| gen_der(r, depth + 1)).collect())
        }
        14 => {
            let n = r.below(4);
            Der::Set((0..n).map(|_| gen_der(r, depth + 1)).collect())
        }
        _ => {
            let n = r.below(4);
            Der::Ctx(
                r.below(31) as u8,
                (0..n).map(|_| gen_der(r, depth + 1)).collect(),
            )
        }
    }
}

fn test_der() {
    assert_eq!(
        encode(&Der::Oid(vec![1, 2, 840, 113549, 1, 1, 11])),
        [
            0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b
        ]
    );
    assert_eq!(encode(&Der::uint(0)), [2, 1, 0]);
    assert_eq!(encode(&Der::uint(127)), [2, 1, 127]);
    assert_eq!(
        encode(&Der::uint(128)),
        [2, 2, 0, 128],
        "positive numbers with the top bit set need a leading 00"
    );
    assert_eq!(encode(&Der::uint(65537)), [2, 3, 1, 0, 1]);
    assert_eq!(encode(&Der::Int(vec![0xff])), [2, 1, 0xff], "-1");
    assert_eq!(
        encode(&Der::Octets(vec![7; 200]))[..3],
        [4, 0x81, 200],
        "long-form length"
    );
    assert_eq!(encode(&Der::Octets(vec![7; 300]))[..4], [4, 0x82, 1, 44]);
    assert_eq!(
        decode(&[0x30, 0x06, 0x02, 0x01, 0x05, 0x01, 0x01, 0xff]).unwrap(),
        Der::Seq(vec![Der::Int(vec![5]), Der::Bool(true)])
    );
    let bad: [(&[u8], &str); 12] = [
        (
            &[0x04, 0x81, 0x01, 0x41],
            "long-form length for a short value",
        ),
        (&[0x04, 0x82, 0x00, 0x90], "leading zero in length"),
        (&[0x30, 0x80, 0x00, 0x00], "indefinite length"),
        (&[0x02, 0x02, 0x00, 0x01], "padded integer"),
        (&[0x02, 0x02, 0xff, 0x80], "padded negative integer"),
        (&[0x02, 0x00], "empty integer"),
        (&[0x01, 0x01, 0x01], "BOOLEAN true must be FF"),
        (
            &[0x03, 0x02, 0x03, 0xff],
            "BIT STRING padding bits not zero",
        ),
        (
            &[0x06, 0x03, 0x2a, 0x80, 0x01],
            "OID subidentifier with leading 0x80",
        ),
        (&[0x04, 0x05, 0x41], "length past end of input"),
        (&[0x05, 0x00, 0x00], "trailing byte"),
        (
            &[0x24, 0x03, 0x04, 0x01, 0x41],
            "constructed OCTET STRING (BER only)",
        ),
    ];
    for (b, why) in bad {
        assert!(decode(b).is_err(), "must reject: {why}");
    }
    let deep = (0..40).fold(vec![0x05, 0x00], |acc, _| tlv(0x30, &acc));
    assert_eq!(decode(&deep), Err(DerError::TooDeep));
    let mut rng = Rng(0xDE12_0001);
    for _ in 0..2000 {
        let d = gen_der(&mut rng, 0);
        let bytes = encode(&d);
        let back = decode(&bytes).unwrap_or_else(|e| panic!("decode failed {e:?} for {d:?}"));
        assert_eq!(back, d);
        assert_eq!(
            encode(&back),
            bytes,
            "DER is canonical: re-encoding is byte-identical"
        );
    }
    println!(
        "-- DER: known encodings OK, 12 non-DER inputs rejected, 2000 random structures round-trip byte-exactly"
    );
}

// ---------------------------------------------------------------------------------------------- time
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
/// UTCTime (YY: 50..99 -> 19YY, else 20YY, per RFC 5280) or GeneralizedTime to Unix seconds.
fn time_to_unix(d: &Der) -> Option<i64> {
    let (s, full) = match d {
        Der::UtcTime(s) => (s.as_str(), false),
        Der::GenTime(s) => (s.as_str(), true),
        _ => return None,
    };
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, rest) = if full {
        (n(0, 4)?, 4)
    } else {
        let yy = n(0, 2)?;
        (if yy >= 50 { 1900 + yy } else { 2000 + yy }, 2)
    };
    let (mo, da, h, mi, se) = (
        n(rest, rest + 2)?,
        n(rest + 2, rest + 4)?,
        n(rest + 4, rest + 6)?,
        n(rest + 6, rest + 8)?,
        n(rest + 8, rest + 10)?,
    );
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    Some(days_from_civil(y, mo, da) * 86400 + h * 3600 + mi * 60 + se)
}
fn utc(y: i64, mo: i64, d: i64) -> i64 {
    days_from_civil(y, mo, d) * 86400
}
fn fmt_time(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86400), t.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}
fn der_time(t: i64) -> Der {
    let s = fmt_time(t).replace(['-', ' ', ':'], "");
    let year: i64 = s[..4].parse().unwrap();
    if (1950..2050).contains(&year) {
        Der::UtcTime(format!("{}Z", &s[2..14]))
    } else {
        Der::GenTime(s)
    }
}

// ======================================================================================================
// (b) X.509
// ======================================================================================================
const OID_CN: [u32; 4] = [2, 5, 4, 3];
const OID_O: [u32; 4] = [2, 5, 4, 10];
const OID_C: [u32; 4] = [2, 5, 4, 6];
const OID_RSA: [u32; 7] = [1, 2, 840, 113549, 1, 1, 1];
const OID_SHA256_RSA: [u32; 7] = [1, 2, 840, 113549, 1, 1, 11];
const OID_BASIC: [u32; 4] = [2, 5, 29, 19];
const OID_KU: [u32; 4] = [2, 5, 29, 15];
const OID_SAN: [u32; 4] = [2, 5, 29, 17];
const KU_DIGITAL_SIGNATURE: u16 = 1 << 0;
const KU_KEY_CERT_SIGN: u16 = 1 << 5;

#[derive(Clone, PartialEq, Debug)]
struct Name(Vec<(Vec<u32>, String)>);
impl Name {
    fn new(cn: &str, o: &str) -> Name {
        Name(vec![
            (OID_C.to_vec(), "US".into()),
            (OID_O.to_vec(), o.into()),
            (OID_CN.to_vec(), cn.into()),
        ])
    }
    fn to_der(&self) -> Der {
        Der::Seq(
            self.0
                .iter()
                .map(|(oid, v)| {
                    let val = if oid == &OID_C {
                        Der::Printable(v.clone())
                    } else {
                        Der::Utf8(v.clone())
                    };
                    Der::Set(vec![Der::Seq(vec![Der::Oid(oid.clone()), val])])
                })
                .collect(),
        )
    }
    fn from_der(d: &Der) -> Result<Name, CertError> {
        let Der::Seq(rdns) = d else {
            return Err(CertError::Structure("name is not a SEQUENCE"));
        };
        let mut out = Vec::new();
        for rdn in rdns {
            let Der::Set(atvs) = rdn else {
                return Err(CertError::Structure("RDN is not a SET"));
            };
            for atv in atvs {
                match atv {
                    Der::Seq(p) => match p.as_slice() {
                        [Der::Oid(o), Der::Utf8(s) | Der::Printable(s) | Der::Ia5(s)] => {
                            out.push((o.clone(), s.clone()))
                        }
                        _ => return Err(CertError::Structure("attribute")),
                    },
                    _ => return Err(CertError::Structure("attribute")),
                }
            }
        }
        Ok(Name(out))
    }
    fn cn(&self) -> Option<&str> {
        self.0
            .iter()
            .find(|(o, _)| o == &OID_CN)
            .map(|x| x.1.as_str())
    }
    fn show(&self) -> String {
        self.0
            .iter()
            .map(|(o, v)| {
                format!(
                    "{}={v}",
                    match o.as_slice() {
                        [2, 5, 4, 3] => "CN",
                        [2, 5, 4, 10] => "O",
                        [2, 5, 4, 6] => "C",
                        _ => "?",
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, PartialEq)]
enum CertError {
    Der(DerError),
    Structure(&'static str),
    UnknownCriticalExtension(String),
}
impl From<DerError> for CertError {
    fn from(e: DerError) -> Self {
        CertError::Der(e)
    }
}

#[derive(Clone, Debug)]
struct Cert {
    der: Vec<u8>,
    tbs: Vec<u8>,
    version: i64,
    serial: Vec<u8>,
    issuer: Name,
    subject: Name,
    not_before: i64,
    not_after: i64,
    pubkey: Vec<u8>,
    ca: Option<(bool, Option<u32>)>, // basicConstraints (cA, pathLenConstraint)
    key_usage: Option<u16>,
    san_dns: Vec<String>,
    signature: Vec<u8>,
}

/// The toy signature scheme. REAL certificates use RSA/ECDSA over the same TBS bytes; the structure is identical.
fn toy_sign(issuer_pub: &[u8], tbs: &[u8]) -> Vec<u8> {
    hmac_sha256(issuer_pub, tbs).to_vec()
}

struct Ca {
    name: Name,
    pubkey: Vec<u8>,
}

struct Spec<'a> {
    subject: Name,
    subject_pub: Vec<u8>,
    serial: u64,
    not_before: i64,
    not_after: i64,
    ca: Option<(bool, Option<u32>)>,
    key_usage: Option<u16>,
    sans: &'a [&'a str],
}

fn ext(oid: &[u32], critical: bool, value: Der) -> Der {
    let mut v = vec![Der::Oid(oid.to_vec())];
    if critical {
        v.push(Der::Bool(true)); // DEFAULT FALSE: DER omits it when false
    }
    v.push(Der::Octets(encode(&value)));
    Der::Seq(v)
}

fn issue(spec: &Spec, issuer: &Ca) -> Vec<u8> {
    let alg = Der::Seq(vec![Der::Oid(OID_SHA256_RSA.to_vec()), Der::Null]);
    let mut exts = Vec::new();
    if let Some((ca, pl)) = spec.ca {
        let mut bc = Vec::new();
        if ca {
            bc.push(Der::Bool(true));
        }
        if let Some(p) = pl {
            bc.push(Der::uint(p as u64));
        }
        exts.push(ext(&OID_BASIC, true, Der::Seq(bc)));
    }
    if let Some(ku) = spec.key_usage {
        // bit 0 is the MOST significant bit of the first octet; DER drops trailing zero bits
        let b = ku.reverse_bits().to_be_bytes();
        let n = if b[1] != 0 { 2 } else { 1 };
        let last = b[n - 1];
        exts.push(ext(
            &OID_KU,
            true,
            Der::BitString {
                unused: last.trailing_zeros().min(7) as u8,
                bytes: b[..n].to_vec(),
            },
        ));
    }
    if !spec.sans.is_empty() {
        exts.push(ext(
            &OID_SAN,
            false,
            Der::Seq(
                spec.sans
                    .iter()
                    .map(|s| Der::CtxPrim(2, s.as_bytes().to_vec()))
                    .collect(),
            ),
        ));
    }
    let tbs = Der::Seq(vec![
        Der::Ctx(0, vec![Der::uint(2)]), // version v3
        Der::uint(spec.serial),
        alg.clone(),
        issuer.name.to_der(),
        Der::Seq(vec![der_time(spec.not_before), der_time(spec.not_after)]),
        spec.subject.to_der(),
        Der::Seq(vec![
            Der::Seq(vec![Der::Oid(OID_RSA.to_vec()), Der::Null]),
            Der::BitString {
                unused: 0,
                bytes: spec.subject_pub.clone(),
            },
        ]),
        Der::Ctx(3, vec![Der::Seq(exts)]),
    ]);
    let sig = toy_sign(&issuer.pubkey, &encode(&tbs));
    encode(&Der::Seq(vec![
        tbs,
        alg,
        Der::BitString {
            unused: 0,
            bytes: sig,
        },
    ]))
}

fn parse_cert(der: &[u8]) -> Result<Cert, CertError> {
    let root = decode(der)?;
    let Der::Seq(top) = &root else {
        return Err(CertError::Structure("certificate is not a SEQUENCE"));
    };
    let [
        tbs @ Der::Seq(t),
        outer_alg @ Der::Seq(_),
        Der::BitString {
            unused: 0,
            bytes: signature,
        },
    ] = top.as_slice()
    else {
        return Err(CertError::Structure("certificate layout"));
    };
    let mut it = t.iter().peekable();
    let version = match it.peek() {
        Some(Der::Ctx(0, v)) => match v.as_slice() {
            [Der::Int(i)] if i.len() == 1 => {
                it.next();
                i[0] as i64 + 1
            }
            _ => return Err(CertError::Structure("version")),
        },
        _ => 1,
    };
    let Some(Der::Int(serial)) = it.next() else {
        return Err(CertError::Structure("serial"));
    };
    let Some(inner_alg @ Der::Seq(_)) = it.next() else {
        return Err(CertError::Structure("signature algorithm"));
    };
    if inner_alg != outer_alg {
        return Err(CertError::Structure("signature algorithm fields differ"));
    }
    let issuer = Name::from_der(it.next().ok_or(CertError::Structure("issuer"))?)?;
    let (not_before, not_after) = match it.next() {
        Some(Der::Seq(v)) if v.len() == 2 => (
            time_to_unix(&v[0]).ok_or(CertError::Structure("notBefore"))?,
            time_to_unix(&v[1]).ok_or(CertError::Structure("notAfter"))?,
        ),
        _ => return Err(CertError::Structure("validity")),
    };
    let subject = Name::from_der(it.next().ok_or(CertError::Structure("subject"))?)?;
    let pubkey = match it.next() {
        Some(Der::Seq(v)) => match v.as_slice() {
            [Der::Seq(_), Der::BitString { bytes, .. }] => bytes.clone(),
            _ => return Err(CertError::Structure("spki")),
        },
        _ => return Err(CertError::Structure("spki")),
    };
    let (mut ca, mut key_usage, mut san_dns) = (None, None, Vec::new());
    for rest in it {
        let Der::Ctx(3, v) = rest else { continue }; // [1],[2] unique IDs ignored
        let [Der::Seq(exts)] = v.as_slice() else {
            return Err(CertError::Structure("extensions"));
        };
        for e in exts {
            let Der::Seq(f) = e else {
                return Err(CertError::Structure("extension"));
            };
            let (oid, critical, val) = match f.as_slice() {
                [Der::Oid(o), Der::Octets(v)] => (o, false, v),
                [Der::Oid(o), Der::Bool(c), Der::Octets(v)] => (o, *c, v),
                _ => return Err(CertError::Structure("extension layout")),
            };
            match oid.as_slice() {
                [2, 5, 29, 19] => {
                    let Der::Seq(items) = decode(val)? else {
                        return Err(CertError::Structure("basicConstraints"));
                    };
                    let is_ca = matches!(items.first(), Some(Der::Bool(true)));
                    let pl = items.iter().find_map(|i| {
                        if let Der::Int(v) = i {
                            Some(v.iter().fold(0u32, |a, &b| a.wrapping_shl(8) | b as u32))
                        } else {
                            None
                        }
                    });
                    ca = Some((is_ca, pl));
                }
                [2, 5, 29, 15] => {
                    let Der::BitString { bytes, .. } = decode(val)? else {
                        return Err(CertError::Structure("keyUsage"));
                    };
                    let mut bits = 0u16;
                    for i in 0..9 {
                        if bytes.get(i / 8).is_some_and(|b| b & (0x80 >> (i % 8)) != 0) {
                            bits |= 1 << i;
                        }
                    }
                    key_usage = Some(bits);
                }
                [2, 5, 29, 17] => {
                    let Der::Seq(names) = decode(val)? else {
                        return Err(CertError::Structure("subjectAltName"));
                    };
                    for n in names {
                        if let Der::CtxPrim(2, b) = n {
                            san_dns.push(
                                String::from_utf8(b)
                                    .map_err(|_| CertError::Structure("SAN not UTF-8"))?,
                            );
                        }
                    }
                }
                other if critical => {
                    return Err(CertError::UnknownCriticalExtension(oid_str(other)));
                }
                _ => {}
            }
        }
    }
    Ok(Cert {
        der: der.to_vec(),
        tbs: encode(tbs),
        version,
        serial: serial.clone(),
        issuer,
        subject,
        not_before,
        not_after,
        pubkey,
        ca,
        key_usage,
        san_dns,
        signature: signature.clone(),
    })
}

/// RFC 6125-style matching: case-insensitive; a wildcard is only allowed as the ENTIRE leftmost label, matches
/// exactly one label, and needs at least two further labels (so `*.com` never matches).
fn hostname_matches(pattern: &str, host: &str) -> bool {
    let (p, h) = (
        pattern.trim_end_matches('.').to_ascii_lowercase(),
        host.trim_end_matches('.').to_ascii_lowercase(),
    );
    let (pl, hl): (Vec<&str>, Vec<&str>) = (p.split('.').collect(), h.split('.').collect());
    if p.is_empty()
        || h.is_empty()
        || pl.len() != hl.len()
        || pl.iter().chain(hl.iter()).any(|l| l.is_empty())
    {
        return false;
    }
    if pl.iter().skip(1).any(|l| l.contains('*')) || hl.iter().any(|l| l.contains('*')) {
        return false;
    }
    if pl[0].contains('*') {
        return pl[0] == "*" && pl.len() >= 3 && pl[1..] == hl[1..];
    }
    pl == hl
}

fn cert_names_match(c: &Cert, host: &str) -> bool {
    if !c.san_dns.is_empty() {
        c.san_dns.iter().any(|p| hostname_matches(p, host)) // SAN present: the CN is ignored
    } else {
        c.subject.cn().is_some_and(|cn| hostname_matches(cn, host))
    }
}

trait Clock {
    fn now(&self) -> i64;
}
struct FixedClock(i64);
impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.0
    }
}

#[derive(Debug, PartialEq)]
enum ChainError {
    Parse(String),
    NotYetValid(usize),
    Expired(usize),
    IssuerMismatch(usize),
    BadSignature(usize),
    NotCa(usize),
    KeyUsage(usize),
    PathLen(usize),
    UnknownRoot,
    NameMismatch,
}

/// `chain` is leaf-first; `roots` are the trust anchors. Certificate i must be issued by i+1 (the last by an anchor).
fn validate_chain(
    chain: &[Cert],
    roots: &[Cert],
    host: &str,
    clock: &dyn Clock,
) -> Result<(), ChainError> {
    let now = clock.now();
    let Some(leaf) = chain.first() else {
        return Err(ChainError::UnknownRoot);
    };
    if !cert_names_match(leaf, host) {
        return Err(ChainError::NameMismatch);
    }
    for (i, c) in chain.iter().enumerate() {
        if now < c.not_before {
            return Err(ChainError::NotYetValid(i));
        }
        if now > c.not_after {
            return Err(ChainError::Expired(i));
        }
        let issuer: &Cert = match chain.get(i + 1) {
            Some(next) => next,
            None => roots
                .iter()
                .find(|r| r.subject == c.issuer)
                .ok_or(ChainError::UnknownRoot)?,
        };
        if issuer.subject != c.issuer {
            return Err(ChainError::IssuerMismatch(i));
        }
        if toy_sign(&issuer.pubkey, &c.tbs) != c.signature {
            return Err(ChainError::BadSignature(i));
        }
        // The issuer must be a CA allowed to sign certificates. Its pathLenConstraint limits how many
        // intermediate CAs may follow it: those are exactly the certificates at chain indices 1..=i.
        match issuer.ca {
            Some((true, pl)) => {
                if pl.is_some_and(|p| i as u32 > p) {
                    return Err(ChainError::PathLen(i + 1));
                }
            }
            _ => return Err(ChainError::NotCa(i + 1)),
        }
        if issuer
            .key_usage
            .is_some_and(|ku| ku & KU_KEY_CERT_SIGN == 0)
        {
            return Err(ChainError::KeyUsage(i + 1));
        }
    }
    Ok(())
}

// ======================================================================================================
// (c) TLS 1.3 handshake model and key schedule
// ======================================================================================================
/// TOY key exchange: finite-field Diffie-Hellman modulo the Mersenne prime 2^61-1 (NOT secure; real TLS uses
/// X25519 or P-256). It only has to give both sides the same shared secret.
const P: u64 = (1u64 << 61) - 1;
fn mulmod(a: u64, b: u64) -> u64 {
    ((a as u128 * b as u128) % P as u128) as u64
}
fn powmod(mut b: u64, mut e: u64) -> u64 {
    let mut r = 1;
    while e > 0 {
        if e & 1 == 1 {
            r = mulmod(r, b);
        }
        b = mulmod(b, b);
        e >>= 1;
    }
    r
}
fn share_bytes(v: u64) -> [u8; 32] {
    let mut s = [0u8; 32];
    s[24..].copy_from_slice(&v.to_be_bytes());
    s
}
fn share_value(s: &[u8; 32]) -> u64 {
    u64::from_be_bytes(s[24..].try_into().unwrap()) % P
}

/// HKDF-Expand-Label (RFC 8446 section 7.1): HkdfLabel = length(u16) || "tls13 "+label (u8-prefixed) || context (u8-prefixed).
fn expand_label(secret: &[u8], label: &str, ctx: &[u8], len: usize) -> Vec<u8> {
    let full = format!("tls13 {label}");
    let mut info = (len as u16).to_be_bytes().to_vec();
    info.push(full.len() as u8);
    info.extend(full.as_bytes());
    info.push(ctx.len() as u8);
    info.extend(ctx);
    hkdf_expand(secret, &info, len)
}
fn derive_secret(secret: &[u8], label: &str, transcript_hash: &[u8; 32]) -> [u8; 32] {
    expand_label(secret, label, transcript_hash, 32)
        .try_into()
        .unwrap()
}

struct Keys {
    c_hs: [u8; 32],
    s_hs: [u8; 32],
    c_ap: [u8; 32],
    s_ap: [u8; 32],
    c_key: Vec<u8>,
    c_iv: Vec<u8>,
    s_key: Vec<u8>,
    s_iv: Vec<u8>,
}

/// early -> handshake secret (mixes in the DH shared secret), bound to Hash(ClientHello..ServerHello).
fn hs_secrets(ecdhe: &[u8; 32], th_ch_sh: &[u8; 32]) -> ([u8; 32], [u8; 32], [u8; 32]) {
    let early = hkdf_extract(&[0u8; 32], &[0u8; 32]); // no PSK: salt and IKM are HashLen zero bytes
    let derived = derive_secret(&early, "derived", &sha256(b""));
    let hs = hkdf_extract(&derived, ecdhe);
    (
        hs,
        derive_secret(&hs, "c hs traffic", th_ch_sh),
        derive_secret(&hs, "s hs traffic", th_ch_sh),
    )
}
/// handshake -> master secret, bound to Hash(ClientHello..server Finished).
fn ap_secrets(hs: &[u8; 32], th_through_sfin: &[u8; 32]) -> ([u8; 32], [u8; 32], [u8; 32]) {
    let derived = derive_secret(hs, "derived", &sha256(b""));
    let master = hkdf_extract(&derived, &[0u8; 32]);
    (
        master,
        derive_secret(&master, "c ap traffic", th_through_sfin),
        derive_secret(&master, "s ap traffic", th_through_sfin),
    )
}
fn finished_mac(base: &[u8; 32], th: &[u8; 32]) -> [u8; 32] {
    hmac_sha256(&expand_label(base, "finished", b"", 32), th)
}

struct Transcript(Sha256);
impl Transcript {
    fn new() -> Self {
        Transcript(Sha256::new())
    }
    fn add(&mut self, m: &[u8]) {
        self.0.update(m);
    }
    fn hash(&self) -> [u8; 32] {
        self.0.clone().finalize() // hash of everything so far, without consuming the running state
    }
}

#[derive(Debug)]
enum HsError {
    Parse(&'static str),
    Unexpected(u8),
    Cert(String),
    BadCertVerify,
    BadFinished(&'static str),
}

struct Rd<'a> {
    b: &'a [u8],
}
impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], HsError> {
        if self.b.len() < n {
            return Err(HsError::Parse("truncated"));
        }
        let (h, t) = self.b.split_at(n);
        self.b = t;
        Ok(h)
    }
    fn u8(&mut self) -> Result<u8, HsError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<usize, HsError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]) as usize)
    }
    fn u24(&mut self) -> Result<usize, HsError> {
        let b = self.take(3)?;
        Ok((b[0] as usize) << 16 | (b[1] as usize) << 8 | b[2] as usize)
    }
    fn vec8(&mut self) -> Result<&'a [u8], HsError> {
        let n = self.u8()? as usize;
        self.take(n)
    }
    fn vec16(&mut self) -> Result<&'a [u8], HsError> {
        let n = self.u16()?;
        self.take(n)
    }
    fn vec24(&mut self) -> Result<&'a [u8], HsError> {
        let n = self.u24()?;
        self.take(n)
    }
    fn done(&self) -> Result<(), HsError> {
        if self.b.is_empty() {
            Ok(())
        } else {
            Err(HsError::Parse("trailing bytes"))
        }
    }
}

fn hs_msg(ty: u8, body: &[u8]) -> Vec<u8> {
    let mut m = vec![
        ty,
        (body.len() >> 16) as u8,
        (body.len() >> 8) as u8,
        body.len() as u8,
    ];
    m.extend(body);
    m
}
fn parse_hs(msg: &[u8]) -> Result<(u8, &[u8]), HsError> {
    let mut r = Rd { b: msg };
    let ty = r.u8()?;
    let body = r.vec24()?;
    r.done()?;
    Ok((ty, body))
}
fn vec16(b: &[u8]) -> Vec<u8> {
    [&(b.len() as u16).to_be_bytes()[..], b].concat()
}
fn ext_item(ty: u16, data: &[u8]) -> Vec<u8> {
    [&ty.to_be_bytes()[..], &vec16(data)].concat()
}
fn parse_exts(b: &[u8]) -> Result<Vec<(usize, &[u8])>, HsError> {
    let mut r = Rd { b };
    let mut out = Vec::new();
    while !r.b.is_empty() {
        let t = r.u16()?;
        out.push((t, r.vec16()?));
    }
    Ok(out)
}
const EXT_SERVER_NAME: u16 = 0;
const EXT_GROUPS: u16 = 10;
const EXT_SIGALGS: u16 = 13;
const EXT_VERSIONS: u16 = 43;
const EXT_KEY_SHARE: u16 = 51;
const TLS_AES_128_GCM_SHA256: u16 = 0x1301;
const GROUP_X25519: u16 = 0x001d;

fn build_client_hello(random: &[u8; 32], sid: &[u8; 32], share: &[u8; 32], host: &str) -> Vec<u8> {
    let sni = vec16(&[&[0u8][..], &vec16(host.as_bytes())].concat());
    let exts = [
        ext_item(EXT_SERVER_NAME, &sni),
        ext_item(EXT_VERSIONS, &[2, 3, 4]),
        ext_item(EXT_GROUPS, &vec16(&GROUP_X25519.to_be_bytes())),
        ext_item(EXT_SIGALGS, &vec16(&0x0804u16.to_be_bytes())),
        ext_item(
            EXT_KEY_SHARE,
            &vec16(&[&GROUP_X25519.to_be_bytes()[..], &vec16(share)].concat()),
        ),
    ]
    .concat();
    let mut body = vec![3, 3];
    body.extend(random);
    body.push(32);
    body.extend(sid);
    body.extend(vec16(&TLS_AES_128_GCM_SHA256.to_be_bytes()));
    body.extend([1, 0]); // one compression method: null
    body.extend(vec16(&exts));
    hs_msg(1, &body)
}
fn parse_client_hello(msg: &[u8]) -> Result<(Vec<u8>, [u8; 32]), HsError> {
    let (ty, body) = parse_hs(msg)?;
    if ty != 1 {
        return Err(HsError::Unexpected(ty));
    }
    let mut r = Rd { b: body };
    if r.take(2)? != [3, 3] {
        return Err(HsError::Parse("legacy_version"));
    }
    r.take(32)?;
    let sid = r.vec8()?.to_vec();
    let suites = r.vec16()?;
    if !suites
        .chunks(2)
        .any(|c| c == TLS_AES_128_GCM_SHA256.to_be_bytes())
    {
        return Err(HsError::Parse("no common cipher suite"));
    }
    r.vec8()?;
    let exts = parse_exts(r.vec16()?)?;
    r.done()?;
    let ks = exts
        .iter()
        .find(|e| e.0 == EXT_KEY_SHARE as usize)
        .ok_or(HsError::Parse("no key_share"))?
        .1;
    let mut k = Rd { b: ks };
    let mut shares = Rd { b: k.vec16()? };
    if shares.u16()? != GROUP_X25519 as usize {
        return Err(HsError::Parse("unsupported group"));
    }
    let share: [u8; 32] = shares
        .vec16()?
        .try_into()
        .map_err(|_| HsError::Parse("share length"))?;
    Ok((sid, share))
}
fn build_server_hello(random: &[u8; 32], sid: &[u8], share: &[u8; 32]) -> Vec<u8> {
    let exts = [
        ext_item(EXT_VERSIONS, &[3, 4]),
        ext_item(
            EXT_KEY_SHARE,
            &[&GROUP_X25519.to_be_bytes()[..], &vec16(share)].concat(),
        ),
    ]
    .concat();
    let mut body = vec![3, 3];
    body.extend(random);
    body.push(sid.len() as u8);
    body.extend(sid);
    body.extend(TLS_AES_128_GCM_SHA256.to_be_bytes());
    body.push(0);
    body.extend(vec16(&exts));
    hs_msg(2, &body)
}
fn parse_server_hello(msg: &[u8]) -> Result<[u8; 32], HsError> {
    let (ty, body) = parse_hs(msg)?;
    if ty != 2 {
        return Err(HsError::Unexpected(ty));
    }
    let mut r = Rd { b: body };
    if r.take(2)? != [3, 3] {
        return Err(HsError::Parse("legacy_version"));
    }
    r.take(32)?;
    r.vec8()?;
    if r.u16()? != TLS_AES_128_GCM_SHA256 as usize || r.u8()? != 0 {
        return Err(HsError::Parse("cipher/compression"));
    }
    let exts = parse_exts(r.vec16()?)?;
    r.done()?;
    if exts
        .iter()
        .find(|e| e.0 == EXT_VERSIONS as usize)
        .map(|e| e.1)
        != Some(&[3, 4])
    {
        return Err(HsError::Parse("not TLS 1.3"));
    }
    let ks = exts
        .iter()
        .find(|e| e.0 == EXT_KEY_SHARE as usize)
        .ok_or(HsError::Parse("no key_share"))?
        .1;
    let mut k = Rd { b: ks };
    if k.u16()? != GROUP_X25519 as usize {
        return Err(HsError::Parse("group"));
    }
    let s: [u8; 32] = k
        .vec16()?
        .try_into()
        .map_err(|_| HsError::Parse("share length"))?;
    k.done()?;
    Ok(s)
}
fn build_certificate(chain: &[Vec<u8>]) -> Vec<u8> {
    let list: Vec<u8> = chain
        .iter()
        .flat_map(|c| {
            [
                &[(c.len() >> 16) as u8, (c.len() >> 8) as u8, c.len() as u8][..],
                c,
                &[0, 0],
            ]
            .concat()
        })
        .collect();
    let mut body = vec![0]; // certificate_request_context: empty in a server's message
    body.extend([
        (list.len() >> 16) as u8,
        (list.len() >> 8) as u8,
        list.len() as u8,
    ]);
    body.extend(list);
    hs_msg(11, &body)
}
fn parse_certificate(msg: &[u8]) -> Result<Vec<Vec<u8>>, HsError> {
    let (ty, body) = parse_hs(msg)?;
    if ty != 11 {
        return Err(HsError::Unexpected(ty));
    }
    let mut r = Rd { b: body };
    r.vec8()?;
    let mut list = Rd { b: r.vec24()? };
    r.done()?;
    let mut out = Vec::new();
    while !list.b.is_empty() {
        out.push(list.vec24()?.to_vec());
        list.vec16()?; // per-certificate extensions
    }
    Ok(out)
}
/// The bytes the server signs in CertificateVerify: 64 spaces, a context string, a zero byte, the transcript hash.
fn cert_verify_content(th: &[u8; 32]) -> Vec<u8> {
    [
        &[0x20u8; 64][..],
        b"TLS 1.3, server CertificateVerify",
        &[0],
        th,
    ]
    .concat()
}

struct ServerCfg {
    chain_der: Vec<Vec<u8>>,
    leaf_pub: Vec<u8>,
}

/// Run a full handshake between an in-process client and server. `tamper` flips one bit of the message with
/// that index as it is "delivered" (0 ClientHello, 1 ServerHello, 2 EncryptedExtensions, 3 Certificate,
/// 4 CertificateVerify, 5 server Finished): a man in the middle.
fn handshake(
    seed: u64,
    tamper: Option<(usize, usize, u8)>,
    cfg: &ServerCfg,
    roots: &[Cert],
    host: &str,
    clock: &dyn Clock,
) -> Result<(Keys, Keys), HsError> {
    let mut rng = Rng(seed | 1);
    let deliver = |i: usize, m: &[u8]| -> Vec<u8> {
        let mut m = m.to_vec();
        if let Some((ti, byte, bit)) = tamper {
            if ti == i {
                let n = m.len();
                m[byte % n] ^= 1 << (bit % 8);
            }
        }
        m
    };
    // ---- client: ClientHello
    let c_priv = rng.next() % (P - 2) + 1;
    let c_share = share_bytes(powmod(3, c_priv));
    let (c_random, sid): ([u8; 32], [u8; 32]) = (
        rng.bytes(32).try_into().unwrap(),
        rng.bytes(32).try_into().unwrap(),
    );
    let ch = build_client_hello(&c_random, &sid, &c_share, host);
    // ---- server: processes ClientHello, answers with ServerHello + the encrypted flight
    let ch_at_server = deliver(0, &ch);
    let (sid_s, peer_share) = parse_client_hello(&ch_at_server)?;
    let s_priv = rng.next() % (P - 2) + 1;
    let s_share = share_bytes(powmod(3, s_priv));
    let s_random: [u8; 32] = rng.bytes(32).try_into().unwrap();
    let sh = build_server_hello(&s_random, &sid_s, &s_share);
    let mut ts_s = Transcript::new();
    ts_s.add(&ch_at_server);
    ts_s.add(&sh);
    let shared_s = share_bytes(powmod(share_value(&peer_share), s_priv));
    let (hs_s, c_hs_s, s_hs_s) = hs_secrets(&shared_s, &ts_s.hash());
    let ee = hs_msg(8, &[0, 0]);
    let cert = build_certificate(&cfg.chain_der);
    ts_s.add(&ee);
    ts_s.add(&cert);
    let sig = toy_sign(&cfg.leaf_pub, &cert_verify_content(&ts_s.hash()));
    let cv = hs_msg(15, &[&0x0804u16.to_be_bytes()[..], &vec16(&sig)].concat());
    ts_s.add(&cv);
    let sfin = hs_msg(20, &finished_mac(&s_hs_s, &ts_s.hash()));
    ts_s.add(&sfin);
    let th_sfin_s = ts_s.hash();
    let (_, c_ap_s, s_ap_s) = ap_secrets(&hs_s, &th_sfin_s);
    // ---- client: processes the server flight
    let flight = [sh, ee, cert, cv, sfin];
    let got: Vec<Vec<u8>> = flight
        .iter()
        .enumerate()
        .map(|(i, m)| deliver(i + 1, m))
        .collect();
    let mut ts_c = Transcript::new();
    ts_c.add(&ch);
    let s_share_c = parse_server_hello(&got[0])?;
    ts_c.add(&got[0]);
    let shared_c = share_bytes(powmod(share_value(&s_share_c), c_priv));
    let (hs_c, c_hs_c, s_hs_c) = hs_secrets(&shared_c, &ts_c.hash());
    let (ty, _) = parse_hs(&got[1])?;
    if ty != 8 {
        return Err(HsError::Unexpected(ty));
    }
    ts_c.add(&got[1]);
    let chain_der = parse_certificate(&got[2])?;
    let chain: Vec<Cert> = chain_der
        .iter()
        .map(|d| parse_cert(d).map_err(|e| HsError::Cert(format!("{e:?}"))))
        .collect::<Result<_, _>>()?;
    validate_chain(&chain, roots, host, clock).map_err(|e| HsError::Cert(format!("{e:?}")))?;
    ts_c.add(&got[2]);
    let (ty, body) = parse_hs(&got[3])?;
    if ty != 15 {
        return Err(HsError::Unexpected(ty));
    }
    let mut r = Rd { b: body };
    if r.u16()? != 0x0804 {
        return Err(HsError::Parse("signature scheme"));
    }
    let sig = r.vec16()?;
    r.done()?;
    if toy_sign(&chain[0].pubkey, &cert_verify_content(&ts_c.hash())) != sig {
        return Err(HsError::BadCertVerify);
    }
    ts_c.add(&got[3]);
    let (ty, body) = parse_hs(&got[4])?;
    if ty != 20 {
        return Err(HsError::Unexpected(ty));
    }
    if body != finished_mac(&s_hs_c, &ts_c.hash()) {
        return Err(HsError::BadFinished(
            "server Finished does not match the client's transcript",
        ));
    }
    ts_c.add(&got[4]);
    let th_sfin_c = ts_c.hash();
    let (_, c_ap_c, s_ap_c) = ap_secrets(&hs_c, &th_sfin_c);
    // client Finished (sent under the client handshake traffic key); the server checks it against ITS transcript
    let cfin = finished_mac(&c_hs_c, &th_sfin_c);
    if cfin != finished_mac(&c_hs_s, &th_sfin_s) {
        return Err(HsError::BadFinished(
            "client Finished does not match the server's transcript",
        ));
    }
    let keys = |c_hs, s_hs, c_ap: [u8; 32], s_ap: [u8; 32]| Keys {
        c_hs,
        s_hs,
        c_key: expand_label(&c_ap, "key", b"", 16),
        c_iv: expand_label(&c_ap, "iv", b"", 12),
        s_key: expand_label(&s_ap, "key", b"", 16),
        s_iv: expand_label(&s_ap, "iv", b"", 12),
        c_ap,
        s_ap,
    };
    Ok((
        keys(c_hs_c, s_hs_c, c_ap_c, s_ap_c),
        keys(c_hs_s, s_hs_s, c_ap_s, s_ap_s),
    ))
}

fn test_key_schedule() {
    // RFC 8448 section 3 ("Simple 1-RTT Handshake"): early secret with no PSK and the "derived" secret from it.
    let early = hkdf_extract(&[0u8; 32], &[0u8; 32]);
    assert_eq!(
        hex(&early),
        "33ad0a1c607ec03b09e6cd9893680ce210adf300aa1f2660e1b22e10f170f92a",
        "RFC 8448 early secret"
    );
    let derived = derive_secret(&early, "derived", &sha256(b""));
    assert_eq!(
        hex(&derived),
        "6f2615a108c702c5678f54fc9dbab69716c076189c48250cebeac3576c3611ba",
        "RFC 8448 derived secret"
    );
    // ... and with the trace's (EC)DHE shared secret the handshake secret must come out as published
    let shared = unhex("8bd4054fb55b9d63fdfbacf9f04b9f0d35e6d63f537563efd46272900f89492d");
    assert_eq!(
        hex(&hkdf_extract(&derived, &shared)),
        "1dc826e93606aa6fdc0aadc12f741b01046aa6b99f691ed221a9f0ca043fbeac",
        "RFC 8448 handshake secret"
    );
    println!(
        "-- key schedule: early/derived/handshake secrets match the RFC 8448 trace byte for byte"
    );
    // HKDF-Expand-Label layout: 00 20 | 0d "tls13 derived" | 20 <hash>
    let mut info = vec![0, 32, 13];
    info.extend(b"tls13 derived");
    info.push(32);
    info.extend(sha256(b""));
    assert_eq!(
        expand_label(&early, "derived", &sha256(b""), 32),
        hkdf_expand(&early, &info, 32)
    );
}

fn test_time_and_names() {
    assert_eq!(utc(1970, 1, 1), 0);
    assert_eq!(utc(2000, 3, 1), 951_868_800);
    assert_eq!(fmt_time(951_868_800 + 3661), "2000-03-01 01:01:01Z");
    assert_eq!(
        time_to_unix(&Der::UtcTime("490101000000Z".into())),
        Some(utc(2049, 1, 1)),
        "YY < 50 means 20YY"
    );
    assert_eq!(
        time_to_unix(&Der::UtcTime("500101000000Z".into())),
        Some(utc(1950, 1, 1)),
        "YY >= 50 means 19YY"
    );
    assert_eq!(
        time_to_unix(&Der::GenTime("20510101000000Z".into())),
        Some(utc(2051, 1, 1))
    );
    assert_eq!(
        time_to_unix(&Der::UtcTime("251301000000Z".into())),
        None,
        "month 13"
    );
    let cases = [
        ("www.example.com", "www.example.com", true),
        ("WWW.Example.COM", "www.example.com", true),
        ("*.example.org", "a.example.org", true),
        ("*.example.org", "example.org", false),
        ("*.example.org", "a.b.example.org", false),
        ("*.com", "example.com", false),
        ("w*.example.org", "www.example.org", false),
        ("a.*.example.org", "a.b.example.org", false),
        ("*.example.org", "*.example.org", false),
        ("*.example.org", "", false),
        ("example.com.", "example.com", true),
    ];
    for (p, h, want) in cases {
        assert_eq!(hostname_matches(p, h), want, "pattern {p:?} host {h:?}");
    }
    println!(
        "-- names/time: {} wildcard and case rules, UTCTime pivot (49 -> 2049, 50 -> 1950), civil-date arithmetic OK",
        cases.len()
    );
}

fn main() {
    test_der();
    test_time_and_names();
    test_key_schedule();

    // ---- build a PKI by hand: root -> intermediate (pathLen 0) -> leaf
    let key = |label: &str| sha256(label.as_bytes()).to_vec(); // "public key" bytes of the toy scheme
    let root = Ca {
        name: Name::new("Example Root CA", "Example Trust"),
        pubkey: key("root"),
    };
    let inter = Ca {
        name: Name::new("Example Intermediate CA", "Example Trust"),
        pubkey: key("inter"),
    };
    let root_der = issue(
        &Spec {
            subject: root.name.clone(),
            subject_pub: root.pubkey.clone(),
            serial: 1,
            not_before: utc(2020, 1, 1),
            not_after: utc(2040, 1, 1),
            ca: Some((true, None)),
            key_usage: Some(KU_KEY_CERT_SIGN),
            sans: &[],
        },
        &root,
    );
    let inter_der = issue(
        &Spec {
            subject: inter.name.clone(),
            subject_pub: inter.pubkey.clone(),
            serial: 0x1234_5678_9abc,
            not_before: utc(2022, 1, 1),
            not_after: utc(2035, 1, 1),
            ca: Some((true, Some(0))),
            key_usage: Some(KU_KEY_CERT_SIGN),
            sans: &[],
        },
        &root,
    );
    let leaf_pub = key("leaf");
    let leaf_der = issue(
        &Spec {
            subject: Name::new("www.example.com", "Example Inc"),
            subject_pub: leaf_pub.clone(),
            serial: 0xdead_beef_cafe,
            not_before: utc(2025, 1, 1),
            not_after: utc(2026, 1, 1),
            ca: Some((false, None)),
            key_usage: Some(KU_DIGITAL_SIGNATURE),
            sans: &["www.example.com", "*.example.org"],
        },
        &inter,
    );
    let (rc, ic, lc) = (
        parse_cert(&root_der).unwrap(),
        parse_cert(&inter_der).unwrap(),
        parse_cert(&leaf_der).unwrap(),
    );
    println!(
        "-- certificates assembled with the DER encoder and parsed back ({} / {} / {} bytes):",
        root_der.len(),
        inter_der.len(),
        leaf_der.len()
    );
    for (label, c) in [("root", &rc), ("intermediate", &ic), ("leaf", &lc)] {
        println!(
            "   {label:<12} v{} serial {} subject [{}] issuer [{}] valid {} .. {} CA {:?} keyUsage {:?} SAN {:?}",
            c.version,
            hex(&c.serial),
            c.subject.show(),
            c.issuer.show(),
            fmt_time(c.not_before),
            fmt_time(c.not_after),
            c.ca,
            c.key_usage.map(|k| format!("{k:#b}")),
            c.san_dns
        );
        assert_eq!(c.version, 3);
    }
    assert_eq!(
        lc.serial,
        vec![0x00, 0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe],
        "serial gets a 00 pad because its top bit is set"
    );
    assert_eq!(rc.issuer, rc.subject, "root is self-issued");
    assert_eq!(
        decode(&leaf_der).map(|d| encode(&d)).unwrap(),
        leaf_der,
        "DER round-trips a whole certificate"
    );

    // ---- chain validation scenarios
    let now = FixedClock(utc(2025, 6, 1));
    let roots = vec![rc.clone()];
    let chain = vec![lc.clone(), ic.clone()];
    assert_eq!(
        validate_chain(&chain, &roots, "www.example.com", &now),
        Ok(())
    );
    assert_eq!(
        validate_chain(&chain, &roots, "foo.example.org", &now),
        Ok(()),
        "wildcard SAN"
    );
    assert_eq!(
        validate_chain(&chain, &roots, "example.org", &now),
        Err(ChainError::NameMismatch)
    );
    assert_eq!(
        validate_chain(&chain, &roots, "evil.example.com", &now),
        Err(ChainError::NameMismatch),
        "CN is ignored when SAN exists"
    );
    assert_eq!(
        validate_chain(
            &chain,
            &roots,
            "www.example.com",
            &FixedClock(utc(2026, 6, 1))
        ),
        Err(ChainError::Expired(0))
    );
    assert_eq!(
        validate_chain(
            &chain,
            &roots,
            "www.example.com",
            &FixedClock(utc(2024, 6, 1))
        ),
        Err(ChainError::NotYetValid(0))
    );
    assert_eq!(
        validate_chain(
            &chain,
            &roots,
            "www.example.com",
            &FixedClock(utc(2036, 1, 1))
        ),
        Err(ChainError::Expired(0))
    );
    assert_eq!(
        validate_chain(&chain, &[], "www.example.com", &now),
        Err(ChainError::UnknownRoot)
    );
    assert_eq!(
        validate_chain(&[lc.clone()], &roots, "www.example.com", &now),
        Err(ChainError::UnknownRoot),
        "missing intermediate"
    );
    // a leaf pretending to be a CA-signed cert but signed by the WRONG key
    let rogue = Ca {
        name: inter.name.clone(),
        pubkey: key("attacker"),
    };
    let forged = parse_cert(&issue(
        &Spec {
            subject: Name::new("www.example.com", "Evil"),
            subject_pub: key("evil"),
            serial: 9,
            not_before: utc(2025, 1, 1),
            not_after: utc(2026, 1, 1),
            ca: None,
            key_usage: None,
            sans: &["www.example.com"],
        },
        &rogue,
    ))
    .unwrap();
    assert_eq!(
        validate_chain(&[forged, ic.clone()], &roots, "www.example.com", &now),
        Err(ChainError::BadSignature(0))
    );
    // issuer is not a CA
    let leaf_ca = Ca {
        name: lc.subject.clone(),
        pubkey: leaf_pub.clone(),
    };
    let child = parse_cert(&issue(
        &Spec {
            subject: Name::new("sub.example.com", "X"),
            subject_pub: key("child"),
            serial: 3,
            not_before: utc(2025, 1, 1),
            not_after: utc(2026, 1, 1),
            ca: None,
            key_usage: None,
            sans: &["sub.example.com"],
        },
        &leaf_ca,
    ))
    .unwrap();
    assert_eq!(
        validate_chain(
            &[child, lc.clone(), ic.clone()],
            &roots,
            "sub.example.com",
            &now
        ),
        Err(ChainError::NotCa(1))
    );
    // pathLen: intermediate with pathLen 0 may not sign another intermediate
    let inter2 = Ca {
        name: Name::new("Sub Intermediate", "Example Trust"),
        pubkey: key("inter2"),
    };
    let inter2_c = parse_cert(&issue(
        &Spec {
            subject: inter2.name.clone(),
            subject_pub: inter2.pubkey.clone(),
            serial: 4,
            not_before: utc(2023, 1, 1),
            not_after: utc(2030, 1, 1),
            ca: Some((true, None)),
            key_usage: Some(KU_KEY_CERT_SIGN),
            sans: &[],
        },
        &inter,
    ))
    .unwrap();
    let leaf2 = parse_cert(&issue(
        &Spec {
            subject: Name::new("deep.example.com", "Example Inc"),
            subject_pub: key("l2"),
            serial: 5,
            not_before: utc(2025, 1, 1),
            not_after: utc(2026, 1, 1),
            ca: None,
            key_usage: None,
            sans: &["deep.example.com"],
        },
        &inter2,
    ))
    .unwrap();
    assert_eq!(
        validate_chain(
            &[leaf2, inter2_c, ic.clone()],
            &roots,
            "deep.example.com",
            &now
        ),
        Err(ChainError::PathLen(2))
    );
    // key usage without keyCertSign
    let weak = Ca {
        name: Name::new("No Sign CA", "X"),
        pubkey: key("weak"),
    };
    let weak_c = parse_cert(&issue(
        &Spec {
            subject: weak.name.clone(),
            subject_pub: weak.pubkey.clone(),
            serial: 6,
            not_before: utc(2023, 1, 1),
            not_after: utc(2030, 1, 1),
            ca: Some((true, None)),
            key_usage: Some(KU_DIGITAL_SIGNATURE),
            sans: &[],
        },
        &root,
    ))
    .unwrap();
    let l3 = parse_cert(&issue(
        &Spec {
            subject: Name::new("w.example.com", "X"),
            subject_pub: key("l3"),
            serial: 7,
            not_before: utc(2025, 1, 1),
            not_after: utc(2026, 1, 1),
            ca: None,
            key_usage: None,
            sans: &["w.example.com"],
        },
        &weak,
    ))
    .unwrap();
    assert_eq!(
        validate_chain(&[l3, weak_c], &roots, "w.example.com", &now),
        Err(ChainError::KeyUsage(1))
    );
    println!(
        "-- chain validation: ok, wildcard SAN, name mismatch, expired, not yet valid, unknown root, missing intermediate, forged signature, non-CA issuer, pathLen, keyUsage: all as expected"
    );

    // every single-bit flip anywhere in the leaf certificate must be caught (parse error or validation error)
    let (mut parse_fail, mut validate_fail) = (0, 0);
    for byte in 0..leaf_der.len() {
        for bit in 0..8 {
            let mut d = leaf_der.clone();
            d[byte] ^= 1 << bit;
            match parse_cert(&d) {
                Err(_) => parse_fail += 1,
                Ok(c) => {
                    let r = validate_chain(&[c, ic.clone()], &roots, "www.example.com", &now);
                    assert!(
                        r.is_err(),
                        "bit flip at byte {byte} bit {bit} went unnoticed"
                    );
                    validate_fail += 1;
                }
            }
        }
    }
    println!(
        "-- all {} single-bit corruptions of the leaf certificate rejected ({parse_fail} by the DER/X.509 parser, {validate_fail} by name/validity/signature checks)",
        leaf_der.len() * 8
    );

    // ---- TLS 1.3 handshake
    let cfg = ServerCfg {
        chain_der: vec![leaf_der.clone(), inter_der.clone()],
        leaf_pub: leaf_pub.clone(),
    };
    let (ck, sk) =
        handshake(42, None, &cfg, &roots, "www.example.com", &now).expect("handshake succeeds");
    assert_eq!(
        (&ck.c_ap, &ck.s_ap, &ck.c_hs, &ck.s_hs),
        (&sk.c_ap, &sk.s_ap, &sk.c_hs, &sk.s_hs),
        "both sides derive identical secrets"
    );
    assert_eq!(
        (&ck.c_key, &ck.c_iv, &ck.s_key, &ck.s_iv),
        (&sk.c_key, &sk.c_iv, &sk.s_key, &sk.s_iv)
    );
    assert_ne!(ck.c_ap, ck.s_ap);
    assert_ne!(ck.c_key, ck.s_key, "directions use different keys");
    println!(
        "-- TLS 1.3 handshake (ClientHello, ServerHello, EncryptedExtensions, Certificate, CertificateVerify, Finished x2):"
    );
    println!("   client_application_traffic_secret_0 = {}", hex(&ck.c_ap));
    println!(
        "   server write key {} iv {} (AES-128-GCM sizes: 16 + 12 bytes)",
        hex(&ck.s_key),
        hex(&ck.s_iv)
    );
    let (ck2, _) = handshake(44, None, &cfg, &roots, "www.example.com", &now).unwrap();
    assert_ne!(
        ck.c_ap, ck2.c_ap,
        "fresh randoms and DH shares give fresh keys per connection"
    );
    // wrong server name / expired certificate / unknown root fail INSIDE the handshake
    for (what, r) in [
        (
            "wrong host name",
            handshake(1, None, &cfg, &roots, "mail.example.net", &now),
        ),
        (
            "expired certificate",
            handshake(
                1,
                None,
                &cfg,
                &roots,
                "www.example.com",
                &FixedClock(utc(2027, 1, 1)),
            ),
        ),
        (
            "untrusted root",
            handshake(1, None, &cfg, &[], "www.example.com", &now),
        ),
    ] {
        match r {
            Err(HsError::Cert(why)) => println!("   {what}: handshake aborted ({why})"),
            _ => panic!("{what} must abort the handshake"),
        }
    }
    // a server that does not hold the leaf private key cannot produce a valid CertificateVerify
    let imposter = ServerCfg {
        chain_der: cfg.chain_der.clone(),
        leaf_pub: key("not-the-leaf-key"),
    };
    assert!(matches!(
        handshake(5, None, &imposter, &roots, "www.example.com", &now),
        Err(HsError::BadCertVerify)
    ));
    println!(
        "   a server replaying the real certificate chain without the key fails CertificateVerify"
    );

    // transcript binding: flip ONE bit in ANY handshake message and the handshake must fail
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    let mut rng = Rng(0x7157_0000_0001);
    let sizes = [
        200usize,
        90,
        4 + 2,
        4 + 3 + 1 + 3 + 3 + leaf_der.len() + inter_der.len(),
        4 + 2 + 2 + 32,
        4 + 32,
    ];
    let mut flips = 0;
    for msg in 0..6 {
        for _ in 0..120 {
            let (byte, bit) = (rng.below(sizes[msg].max(1) * 4), rng.below(8) as u8);
            match handshake(
                7 + flips as u64,
                Some((msg, byte, bit)),
                &cfg,
                &roots,
                "www.example.com",
                &now,
            ) {
                Ok(_) => {
                    panic!("a bit flip in message {msg} (byte {byte}, bit {bit}) was not detected")
                }
                Err(e) => {
                    let s = format!("{e:?}");
                    *kinds
                        .entry(
                            s.split(|c| c == '(' || c == ' ')
                                .next()
                                .unwrap()
                                .to_string(),
                        )
                        .or_default() += 1;
                }
            }
            flips += 1;
        }
    }
    println!(
        "   {flips} one-bit tamperings (client hello, server hello, EE, Certificate, CertificateVerify, Finished) all failed the handshake: {kinds:?}"
    );
    // the Finished MAC itself: same keys, one bit different transcript
    let th = sha256(b"transcript");
    let base = [9u8; 32];
    let mac = finished_mac(&base, &th);
    for bit in 0..256 {
        let mut t = th;
        t[bit / 8] ^= 1 << (bit % 8);
        assert_ne!(finished_mac(&base, &t), mac);
    }
    println!(
        "   Finished = HMAC(finished_key, transcript hash): all 256 single-bit changes of the hash change the MAC"
    );

    // ---- fuzz the DER decoder and the certificate parser
    let mut rng = Rng(0xF022_5EED);
    let (mut ok, mut err) = (0u32, 0u32);
    for i in 0..30_000 {
        let input: Vec<u8> = match i % 4 {
            0 => {
                let n = rng.below(60);
                rng.bytes(n)
            }
            1 => {
                // plausible header: SEQUENCE with a (possibly wrong) length, then random content
                let n = rng.below(40);
                let mut v = vec![0x30, rng.below(60) as u8];
                v.extend(rng.bytes(n));
                v
            }
            2 => encode(&gen_der(&mut rng, 0)),
            _ => {
                let mut d =
                    [leaf_der.clone(), inter_der.clone(), root_der.clone()][rng.below(3)].clone();
                for _ in 0..1 + rng.below(4) {
                    let j = rng.below(d.len());
                    d[j] = rng.next() as u8;
                }
                if rng.below(4) == 0 {
                    let cut = rng.below(d.len());
                    d.truncate(cut);
                }
                let _ = parse_cert(&d); // the whole X.509 layer must not panic either
                d
            }
        };
        match decode(&input) {
            Ok(_) => ok += 1,
            Err(_) => err += 1,
        }
    }
    println!(
        "-- fuzz: 30000 inputs (random, SEQUENCE-shaped, valid, mutated certificates) -> {ok} decoded, {err} rejected, 0 panics"
    );
    assert_eq!(ok + err, 30_000);
    assert!(ok > 5000 && err > 5000);
    println!("tls_handshake_x509_der: all checks passed");
}
