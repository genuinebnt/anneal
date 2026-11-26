//! RECIPE 185 (encoding): from-scratch codecs: hex, base64/32, percent-encoding, UTF-8/16, varints, checksums.
//! LEVEL: intermediate
//! Use in: wire formats, tokens/URLs, file formats, protocol parsers.   Run: cargo run --release --example encodings_codecs
//!
//! WHAT: the small encodings every backend meets. Each is a pure function bytes <-> text/bytes, so each is
//! pinned by (1) reference vectors (RFC 4648 "foobar"), (2) round-trip fuzzing on seeded random input, and
//! (3) for decoders, a STRICTNESS property: if `decode(s)` succeeds then `encode(decode(s)) == s`. Lenient
//! decoders (accepting stray padding, non-zero trailing bits, overlong UTF-8) are a classic source of
//! signature-bypass and smuggling bugs because two different strings mean the same bytes.
//! HOW: base64 and base32 share one bit-accumulator codec parameterised by alphabet (6 vs 5 bits per char);
//! padding is explicit (`pad: bool`). UTF-8 is validated against `std::str::from_utf8` on 100k random
//! byte strings; UTF-16 against `char::decode_utf16`. Zigzag maps signed to unsigned so small magnitudes stay
//! small (0,-1,1,-2 -> 0,1,2,3) before LEB128 (7 bits/byte, high bit = continue).
//!
//! COMMON USES: base64 in JWTs, data: URLs, MIME and PEM; base32 in TOTP secrets and some content ids;
//! percent-encoding of URL components; LEB128/zigzag varints in protobuf and WebAssembly; Adler-32 in zlib,
//! CRC32 in gzip/PNG/Ethernet. Real code: the `base64`, `hex`, `percent-encoding` crates, and std for UTF-8/16.
//! Read: https://www.rfc-editor.org/rfc/rfc4648   https://www.rfc-editor.org/rfc/rfc3986   https://www.rfc-editor.org/rfc/rfc3629
#![allow(dead_code)]

#[derive(Debug, PartialEq)]
enum CodecError {
    BadChar,
    BadLength,
    BadPadding,
    NonCanonical,
    BadLead,
    BadContinuation,
    Truncated,
    Overlong,
    Surrogate,
    OutOfRange,
    Overflow,
}
use CodecError::*;

// ---------- hex ----------
fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}
fn hex_val(c: u8) -> Result<u8, CodecError> {
    (c as char).to_digit(16).map(|d| d as u8).ok_or(BadChar)
}
fn hex_decode(s: &str) -> Result<Vec<u8>, CodecError> {
    if s.len() % 2 != 0 {
        return Err(BadLength);
    }
    s.as_bytes()
        .chunks(2)
        .map(|p| Ok(hex_val(p[0])? << 4 | hex_val(p[1])?))
        .collect()
}

// ---------- base64 / base32 via one bit accumulator ----------
struct Alphabet {
    chars: &'static [u8],
    bits: u32,    // bits per character: 6 for base64, 5 for base32
    group: usize, // padded output is a multiple of this many chars: 4 / 8
}
const B64: Alphabet = Alphabet {
    chars: b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
    bits: 6,
    group: 4,
};
const B64URL: Alphabet = Alphabet {
    chars: b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    bits: 6,
    group: 4,
};
const B32: Alphabet = Alphabet {
    chars: b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
    bits: 5,
    group: 8,
};

fn encode_with(a: &Alphabet, data: &[u8], pad: bool) -> String {
    let (mut out, mut acc, mut nb) = (String::new(), 0u32, 0u32);
    let mask = (1u32 << a.bits) - 1;
    for &b in data {
        acc = acc << 8 | b as u32;
        nb += 8;
        while nb >= a.bits {
            nb -= a.bits;
            out.push(a.chars[(acc >> nb & mask) as usize] as char);
        }
        acc &= (1 << nb) - 1; // keep only the unconsumed bits so the accumulator never overflows
    }
    if nb > 0 {
        out.push(a.chars[(acc << (a.bits - nb) & mask) as usize] as char); // zero-fill the last char
    }
    while pad && out.len() % a.group != 0 {
        out.push('=');
    }
    out
}

fn decode_with(a: &Alphabet, s: &str, pad: bool) -> Result<Vec<u8>, CodecError> {
    let mut chars = s.as_bytes();
    if pad {
        if chars.len() % a.group != 0 {
            return Err(BadLength);
        }
        let p = chars.iter().rev().take_while(|&&c| c == b'=').count();
        chars = &chars[..chars.len() - p];
        if p != (a.group - chars.len() % a.group) % a.group {
            return Err(BadPadding); // exactly the canonical amount, no more, no less
        }
    }
    let (mut out, mut acc, mut nb) = (Vec::new(), 0u32, 0u32);
    for &c in chars {
        let v = a.chars.iter().position(|&x| x == c).ok_or(BadChar)? as u32; // '=' mid-string lands here
        acc = acc << a.bits | v;
        nb += a.bits;
        if nb >= 8 {
            nb -= 8;
            out.push((acc >> nb) as u8);
            acc &= (1 << nb) - 1;
        }
    }
    if nb >= a.bits {
        return Err(BadLength); // a whole char's worth of leftover bits: impossible length
    }
    if acc != 0 {
        return Err(NonCanonical); // leftover bits must be zero or two strings decode to the same bytes
    }
    Ok(out)
}

// ---------- percent-encoding (RFC 3986 unreserved set) ----------
fn pct_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for &b in data {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
fn pct_decode(s: &str) -> Result<Vec<u8>, CodecError> {
    let (b, mut out, mut i) = (s.as_bytes(), Vec::new(), 0);
    while i < b.len() {
        if b[i] == b'%' {
            let h = b.get(i + 1..i + 3).ok_or(Truncated)?;
            out.push(hex_val(h[0])? << 4 | hex_val(h[1])?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(out)
}

// ---------- UTF-8 ----------
fn utf8_encode(cp: u32) -> Result<Vec<u8>, CodecError> {
    Ok(match cp {
        0xD800..=0xDFFF => return Err(Surrogate),
        0x110000.. => return Err(OutOfRange),
        0..=0x7F => vec![cp as u8],
        0x80..=0x7FF => vec![0xC0 | (cp >> 6) as u8, 0x80 | (cp & 0x3F) as u8],
        0x800..=0xFFFF => vec![
            0xE0 | (cp >> 12) as u8,
            0x80 | (cp >> 6 & 0x3F) as u8,
            0x80 | (cp & 0x3F) as u8,
        ],
        _ => vec![
            0xF0 | (cp >> 18) as u8,
            0x80 | (cp >> 12 & 0x3F) as u8,
            0x80 | (cp >> 6 & 0x3F) as u8,
            0x80 | (cp & 0x3F) as u8,
        ],
    })
}
fn utf8_decode(b: &[u8]) -> Result<Vec<u32>, CodecError> {
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        // (continuation bytes, smallest code point this length may encode, payload bits of the lead byte)
        let (n, min, lead) = match b[i] {
            0x00..=0x7F => (0, 0, b[i] as u32),
            0xC0..=0xDF => (1, 0x80, b[i] as u32 & 0x1F),
            0xE0..=0xEF => (2, 0x800, b[i] as u32 & 0x0F),
            0xF0..=0xF7 => (3, 0x10000, b[i] as u32 & 0x07),
            _ => return Err(BadLead), // stray continuation byte or 0xF8..
        };
        let tail = b.get(i + 1..i + 1 + n).ok_or(Truncated)?;
        let mut cp = lead;
        for &c in tail {
            if c & 0xC0 != 0x80 {
                return Err(BadContinuation);
            }
            cp = cp << 6 | (c & 0x3F) as u32;
        }
        if cp < min {
            return Err(Overlong); // e.g. C0 80 for NUL: a classic filter-bypass trick
        }
        if (0xD800..=0xDFFF).contains(&cp) {
            return Err(Surrogate);
        }
        if cp > 0x10FFFF {
            return Err(OutOfRange);
        }
        out.push(cp);
        i += 1 + n;
    }
    Ok(out)
}

// ---------- UTF-16 ----------
fn utf16_encode(cps: &[u32]) -> Result<Vec<u16>, CodecError> {
    let mut out = Vec::new();
    for &cp in cps {
        match cp {
            0xD800..=0xDFFF => return Err(Surrogate),
            0x110000.. => return Err(OutOfRange),
            0..=0xFFFF => out.push(cp as u16),
            _ => {
                let v = cp - 0x10000; // 20 bits split into two 10-bit halves
                out.push(0xD800 + (v >> 10) as u16);
                out.push(0xDC00 + (v & 0x3FF) as u16);
            }
        }
    }
    Ok(out)
}
fn utf16_decode(u: &[u16]) -> Result<Vec<u32>, CodecError> {
    let (mut out, mut i) = (Vec::new(), 0);
    while i < u.len() {
        match u[i] {
            hi @ 0xD800..=0xDBFF => match u.get(i + 1) {
                Some(&lo @ 0xDC00..=0xDFFF) => {
                    out.push(0x10000 + ((hi as u32 - 0xD800) << 10) + (lo as u32 - 0xDC00));
                    i += 1;
                }
                _ => return Err(Surrogate), // high surrogate without its partner
            },
            0xDC00..=0xDFFF => return Err(Surrogate),
            c => out.push(c as u32),
        }
        i += 1;
    }
    Ok(out)
}

// ---------- zigzag + LEB128 ----------
fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}
fn unzigzag(n: u64) -> i64 {
    (n >> 1) as i64 ^ -((n & 1) as i64)
}
fn leb_encode(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let b = (n & 0x7F) as u8;
        n >>= 7;
        if n == 0 {
            return out.push(b);
        }
        out.push(b | 0x80);
    }
}
/// Strict: at most 10 bytes, the 10th may only carry bit 63, and no redundant trailing zero groups.
fn leb_decode(buf: &[u8]) -> Result<(u64, usize), CodecError> {
    let mut v = 0u64;
    for (i, &b) in buf.iter().enumerate().take(10) {
        if i == 9 && b > 1 {
            return Err(Overflow);
        }
        v |= ((b & 0x7F) as u64) << (7 * i);
        if b & 0x80 == 0 {
            return if b == 0 && i > 0 {
                Err(NonCanonical)
            } else {
                Ok((v, i + 1))
            };
        }
    }
    Err(if buf.len() >= 10 { Overflow } else { Truncated })
}

// ---------- checksums ----------
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521; // largest prime below 2^16
        b = (b + a) % 65521;
    }
    b << 16 | a
}
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &x in data {
        crc ^= x as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

// ---------- tests ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn bytes(&mut self, max: u64) -> Vec<u8> {
        (0..self.below(max)).map(|_| self.next() as u8).collect()
    }
}

fn rfc4648_vectors() {
    let b64 = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];
    let b32 = [
        ("", ""),
        ("f", "MY======"),
        ("fo", "MZXQ===="),
        ("foo", "MZXW6==="),
        ("foob", "MZXW6YQ="),
        ("fooba", "MZXW6YTB"),
        ("foobar", "MZXW6YTBOI======"),
    ];
    for (raw, enc) in b64 {
        assert_eq!(encode_with(&B64, raw.as_bytes(), true), enc);
        assert_eq!(decode_with(&B64, enc, true).unwrap(), raw.as_bytes());
        assert_eq!(
            encode_with(&B64, raw.as_bytes(), false),
            enc.trim_end_matches('=')
        );
    }
    for (raw, enc) in b32 {
        assert_eq!(encode_with(&B32, raw.as_bytes(), true), enc);
        assert_eq!(decode_with(&B32, enc, true).unwrap(), raw.as_bytes());
    }
    assert_eq!(hex_encode(b"foobar"), "666f6f626172");
    assert_eq!(hex_decode("666F6f626172").unwrap(), b"foobar");
    // standard vs URL-safe differ only in the last two alphabet symbols
    assert_eq!(encode_with(&B64, &[0xFB, 0xFF], true), "+/8=");
    assert_eq!(encode_with(&B64URL, &[0xFB, 0xFF], true), "-_8=");
    assert_eq!(pct_encode("a b/c?d=é".as_bytes()), "a%20b%2Fc%3Fd%3D%C3%A9");
}

fn strictness() {
    assert_eq!(decode_with(&B64, "Zg=", true), Err(BadLength));
    assert_eq!(decode_with(&B64, "Zg", true), Err(BadLength)); // padding required
    assert_eq!(decode_with(&B64, "Zg===", true), Err(BadLength));
    assert_eq!(decode_with(&B64, "Z===", true), Err(BadLength));
    assert_eq!(decode_with(&B64, "Zh==", true), Err(NonCanonical)); // 'h' sets a bit that must be zero
    assert_eq!(decode_with(&B64, "Zg=a", true), Err(BadChar));
    assert_eq!(decode_with(&B64, "Zm 9", true), Err(BadChar));
    assert_eq!(decode_with(&B64, "Zg==", false), Err(BadChar)); // padding forbidden in unpadded mode
    assert_eq!(decode_with(&B64, "Z", false), Err(BadLength));
    assert_eq!(decode_with(&B64, "-_8", false), Err(BadChar)); // URL-safe chars are not standard
    assert_eq!(decode_with(&B64URL, "-_8", false).unwrap(), [0xFB, 0xFF]);
    assert_eq!(hex_decode("abc"), Err(BadLength));
    assert_eq!(hex_decode("zz"), Err(BadChar));
    assert_eq!(pct_decode("%4"), Err(Truncated));
    assert_eq!(pct_decode("%4G"), Err(BadChar));
}

fn fuzz_text_codecs(rng: &mut Rng) {
    for _ in 0..3000 {
        let data = rng.bytes(48);
        for pad in [true, false] {
            for alpha in [&B64, &B64URL, &B32] {
                let s = encode_with(alpha, &data, pad);
                assert_eq!(decode_with(alpha, &s, pad).unwrap(), data);
            }
        }
        assert_eq!(hex_decode(&hex_encode(&data)).unwrap(), data);
        assert_eq!(pct_decode(&pct_encode(&data)).unwrap(), data);
    }
    // Strictness property on garbage: anything that decodes must re-encode to the very same text.
    let junk = b"ABCZaz059+/-_=== ABCDEFGH234567";
    let (mut accepted, mut rejected) = (0, 0);
    for _ in 0..200_000 {
        let s: String = (0..rng.below(12))
            .map(|_| junk[rng.below(junk.len() as u64) as usize] as char)
            .collect();
        for (alpha, pad) in [
            (&B64, true),
            (&B64, false),
            (&B64URL, false),
            (&B32, true),
            (&B32, false),
        ] {
            match decode_with(alpha, &s, pad) {
                Ok(d) => {
                    assert_eq!(
                        encode_with(alpha, &d, pad),
                        s,
                        "non-canonical input accepted"
                    );
                    accepted += 1;
                }
                Err(_) => rejected += 1,
            }
        }
    }
    println!(
        "strict decoders: {accepted} garbage strings accepted (all canonical), {rejected} rejected"
    );
}

fn fuzz_unicode(rng: &mut Rng) {
    // Every scalar value: our encoder equals std's, our decoder inverts it. Surrogates/out-of-range are errors.
    let mut buf = [0u8; 4];
    for cp in (0..=0x10FFFFu32)
        .step_by(7)
        .chain([0x7F, 0x80, 0x7FF, 0x800, 0xFFFF, 0x10000, 0x10FFFF])
    {
        match char::from_u32(cp) {
            Some(c) => {
                assert_eq!(utf8_encode(cp).unwrap(), c.encode_utf8(&mut buf).as_bytes());
                assert_eq!(utf8_decode(&utf8_encode(cp).unwrap()).unwrap(), [cp]);
                let mut u16buf = [0u16; 2];
                assert_eq!(utf16_encode(&[cp]).unwrap(), c.encode_utf16(&mut u16buf));
            }
            None => assert_eq!(
                utf8_encode(cp),
                Err(if cp >= 0x110000 {
                    OutOfRange
                } else {
                    Surrogate
                })
            ),
        }
    }
    assert_eq!(utf8_decode(&[0xC0, 0x80]), Err(Overlong));
    assert_eq!(utf8_decode(&[0xE0, 0x80, 0x80]), Err(Overlong));
    assert_eq!(utf8_decode(&[0xED, 0xA0, 0x80]), Err(Surrogate));
    assert_eq!(utf8_decode(&[0xF4, 0x90, 0x80, 0x80]), Err(OutOfRange));
    assert_eq!(utf8_decode(&[0xE2, 0x82]), Err(Truncated));
    assert_eq!(utf8_decode(&[0x80]), Err(BadLead));
    // Differential: 100k byte strings biased toward interesting lead/continuation bytes.
    const SPICY: [u8; 20] = [
        0x00, 0x41, 0x7F, 0x80, 0xBF, 0x9F, 0xA0, 0x8F, 0x90, 0xC0, 0xC1, 0xC2, 0xDF, 0xE0, 0xED,
        0xEF, 0xF0, 0xF4, 0xF5, 0xFF,
    ];
    let mut valid = 0;
    for _ in 0..100_000 {
        let n = rng.below(7) as usize;
        let b: Vec<u8> = (0..n)
            .map(|_| {
                if rng.below(4) == 0 {
                    rng.next() as u8
                } else {
                    SPICY[rng.below(20) as usize]
                }
            })
            .collect();
        match (std::str::from_utf8(&b), utf8_decode(&b)) {
            (Ok(s), Ok(cps)) => {
                assert!(s.chars().map(|c| c as u32).eq(cps));
                valid += 1;
            }
            (Err(_), Err(_)) => {}
            (a, b2) => panic!("disagree on {b:02x?}: std={a:?} ours={b2:?}"),
        }
    }
    // UTF-16 vs std, with units biased toward surrogates.
    for _ in 0..50_000 {
        let u: Vec<u16> = (0..rng.below(5))
            .map(|_| [0x41, 0xD7FF, 0xD800, 0xDBFF, 0xDC00, 0xDFFF, 0xE000][rng.below(7) as usize])
            .collect();
        let std_res: Result<Vec<u32>, _> = char::decode_utf16(u.iter().copied())
            .map(|r| r.map(|c| c as u32))
            .collect();
        assert_eq!(std_res.is_ok(), utf16_decode(&u).is_ok(), "{u:04x?}");
        if let (Ok(a), Ok(b)) = (std_res, utf16_decode(&u)) {
            assert_eq!(a, b);
            assert_eq!(utf16_encode(&b).unwrap(), u);
        }
    }
    println!("utf-8 differential: {valid} of 100000 strings were valid and all agreed with std");
}

fn varints_and_checksums(rng: &mut Rng) {
    let mut cases: Vec<i64> = vec![0, 1, -1, 63, -64, 64, -65, i64::MAX, i64::MIN, i64::MIN + 1];
    cases
        .extend((0..2000).map(|_| {
            (rng.next() >> rng.below(64)) as i64 * if rng.below(2) == 0 { 1 } else { -1 }
        }));
    for &n in &cases {
        assert_eq!(unzigzag(zigzag(n)), n);
        let mut b = Vec::new();
        leb_encode(zigzag(n), &mut b);
        assert_eq!(leb_decode(&b), Ok((zigzag(n), b.len())));
        let mut u = Vec::new();
        leb_encode(n as u64, &mut u);
        assert_eq!(leb_decode(&u), Ok((n as u64, u.len())));
    }
    assert_eq!([0, -1, 1, -2, 2].map(zigzag), [0, 1, 2, 3, 4]);
    let mut max = Vec::new();
    leb_encode(u64::MAX, &mut max);
    assert_eq!(max.len(), 10);
    assert_eq!(leb_decode(&[0x80, 0x00]), Err(NonCanonical));
    assert_eq!(leb_decode(&[0x80]), Err(Truncated));
    assert_eq!(leb_decode(&[0xFF; 10]), Err(Overflow));
    assert_eq!(
        leb_decode(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02]),
        Err(Overflow)
    );

    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    assert_eq!(adler32(b""), 1);
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
    assert_eq!(
        crc32(b"The quick brown fox jumps over the lazy dog"),
        0x414F_A339
    );
}

fn main() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    rfc4648_vectors();
    strictness();
    fuzz_text_codecs(&mut rng);
    fuzz_unicode(&mut rng);
    varints_and_checksums(&mut rng);
    println!(
        "base64(\"foobar\") = {}",
        encode_with(&B64, b"foobar", true)
    );
    println!(
        "base32(\"foobar\") = {}",
        encode_with(&B32, b"foobar", true)
    );
    println!("percent(\"a b/c\")  = {}", pct_encode(b"a b/c"));
    println!("zigzag(-3)={} leb128(300)={:02x?}", zigzag(-3), {
        let mut v = Vec::new();
        leb_encode(300, &mut v);
        v
    });
    println!("ok");
}
