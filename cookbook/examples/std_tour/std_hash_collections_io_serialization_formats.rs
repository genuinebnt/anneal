//! RECIPE 1069 (std_tour): hand-rolled serialisation on `std::io`: endian integers, varints, length-prefixed records, checksums, versioned headers, hostile input.
//! LEVEL: advanced
//! Use in: snapshot and log files, simple wire protocols, cache files, anywhere a serde dependency is not wanted or the byte layout must be exact.
//! Run: cargo run --release --example std_hash_collections_io_serialization_formats
//! COMMON USES: persisting a map to disk, appending records to a log, framing messages, escaping a line-based text export.
//! METHODS: Read::read_exact, Read::read, Read::take, Read::by_ref, Read::read_to_end, Write::write_all, Write::flush, u16::to_le_bytes,
//! u32::from_le_bytes, u32::to_be_bytes, i64::from_be_bytes, String::from_utf8, std::io::ErrorKind::UnexpectedEof,
//! std::error::Error::source, From (for `?`), HashMap, BTreeMap, slice::sort_unstable_by
//! DOCS: https://doc.rust-lang.org/std/io/trait.Read.html
//! DEFINED IN: core: the integer `to_*_bytes`/`from_*_bytes` methods, `str::from_utf8`. alloc: `String`, `Vec`, `BTreeMap`. std: `Read`, `Write`
//!   (`std::io`), `HashMap`, `DefaultHasher`. std has NO serialisation framework and NO checksum function: CRC-32 is written out below.
//! NOT COVERED (and why): serde/bincode/protobuf (see the crate tour); zero-copy slice parsing, TLV skipping and bit readers (see
//! `binary_format_parsing`, which this file complements with the STREAMING side); compression and encryption; schema evolution beyond a version number.
//! SCOPE OF CLAIMS: CRC-32 detects accidental corruption, NOT tampering (anyone can recompute it): use a MAC for that. The fuzz section checks
//! "no panic, no oversized buffer" on seeded random inputs; it is not coverage-guided fuzzing (use cargo-fuzz for that).
//!
//! FORMAT (all integers little endian):
//!   header  = "FFKV" | version u16 | flags u16 (must be 0) | crc32(previous 8 bytes) u32
//!   record  = tag u8 (1 = entry, 0 = end) | len varint | payload[len] | crc32(tag, len, payload) u32
//!   entry   = key_len varint | key (UTF-8) | value i64
//!   end     = number of entries, varint        (without it, a file cut at a record boundary would look complete)
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   integer to bytes and back                    -> `x.to_le_bytes()`, `u32::from_le_bytes(buf)`; read with `read_exact(&mut buf)`
//!   choose a byte order                          -> pick one, write it in the spec; `to_ne_bytes` is wrong for files and networks
//!   read N bytes where N comes from the input    -> check N against a limit, then `r.by_ref().take(N).read_to_end(&mut v)` and compare lengths
//!   tell "clean end" from "cut off"              -> a first `read` returning 0 at a boundary vs EOF inside a record (retry `Interrupted` yourself)
//!   same data => same bytes                      -> sort HashMap entries (or use BTreeMap) before writing
//!   persist a checksum or hash                   -> a specified algorithm (CRC-32, SHA-256); never `DefaultHasher` (unspecified, may change)
//!   evolve the format                            -> magic + version first; reject unknown versions and unknown mandatory features
//!   many small writes                            -> wrap in `BufWriter` and flush explicitly
//!
//! PITFALLS: `Vec::with_capacity(len_from_input)` lets 10 bytes of input demand gigabytes; `read` may return fewer bytes than asked;
//! `as u32` silently truncates a length; a decoder that accepts several encodings of one value breaks hashing and signatures.
//! TRY NEXT:
//!   - [S] Replace the canonical check in `read_varint` by nothing and find which test fails; then explain why signatures care.
//!   - [S] Drop the end record and rerun the truncation sweep. Which cut positions now decode successfully?
//!   - [M] Add version 2 with a per-entry timestamp while still reading version 1 files; keep one golden byte string per version.
//!   - [M] Make `decode` resynchronise after a corrupt record. What must change in the format to make this reliable?
//!   - [A] PROJECT: combine with `std_fs_file_operations_usecases`: an append-only store that recovers by truncating at the first bad record, with a crash test that cuts the file at every offset.
//! Read: https://protobuf.dev/programming-guides/encoding/ (varints) ; https://www.w3.org/TR/png/#D-CRCAppendix (CRC-32 reference code)
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::{self, Read, Write};

// ================================================================ CRC-32 (reflected, polynomial 0xEDB88320: the zlib/PNG one)
const fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}
const CRC_TABLE: [u32; 256] = crc_table(); // computed at compile time

#[derive(Clone, Copy)]
struct Crc32(u32);
impl Crc32 {
    fn new() -> Self {
        Crc32(!0)
    }
    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = CRC_TABLE[usize::from(self.0 as u8 ^ b)] ^ (self.0 >> 8);
        }
    }
    fn finish(self) -> u32 {
        !self.0
    }
    fn of(bytes: &[u8]) -> u32 {
        let mut c = Crc32::new();
        c.update(bytes);
        c.finish()
    }
}

/// Checksums exactly the bytes that really went through, even with short reads and writes.
struct Checked<T> {
    inner: T,
    crc: Crc32,
}
impl<R: Read> Read for Checked<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.crc.update(&buf[..n]);
        Ok(n)
    }
}
impl<W: Write> Write for Checked<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.crc.update(&buf[..n]); // only what was accepted: write_all will offer the rest again
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

// ================================================================ errors
#[derive(Debug)]
enum DecodeError {
    Io(io::Error),
    Truncated,
    BadMagic,
    UnsupportedVersion(u16),
    UnknownFlags(u16),
    BadChecksum,
    BadVarint,
    TooLarge { claimed: u64, limit: u64 },
    UnknownTag(u8),
    BadPayload(&'static str),
    TrailingData,
}
impl From<io::Error> for DecodeError {
    fn from(e: io::Error) -> Self {
        // EOF in the middle of a structure is a property of the DATA; anything else is a real I/O failure
        if e.kind() == io::ErrorKind::UnexpectedEof {
            DecodeError::Truncated
        } else {
            DecodeError::Io(e)
        }
    }
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Io(_) => f.write_str("i/o error while decoding"), // the cause is exposed through source()
            other => write!(f, "invalid data: {other:?}"),
        }
    }
}
impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DecodeError::Io(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, DecodeError>;

// ================================================================ primitives
fn read_array<const N: usize>(r: &mut impl Read) -> Result<[u8; N]> {
    let mut b = [0u8; N];
    r.read_exact(&mut b)?;
    Ok(b)
}

/// LEB128: 7 bits per byte, least significant group first, high bit = "more follows".
fn write_varint(w: &mut impl Write, mut v: u64) -> io::Result<()> {
    let mut buf = [0u8; 10];
    let mut n = 0;
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        buf[n] = byte | if v != 0 { 0x80 } else { 0 };
        n += 1;
        if v == 0 {
            return w.write_all(&buf[..n]);
        }
    }
}

fn read_varint(r: &mut impl Read) -> Result<u64> {
    let mut v = 0u64;
    for i in 0..10 {
        let [b] = read_array::<1>(r)?;
        if i == 9 && b > 1 {
            return Err(DecodeError::BadVarint); // would not fit in 64 bits (also covers a continuation bit)
        }
        v |= u64::from(b & 0x7f) << (7 * i);
        if b & 0x80 == 0 {
            if i > 0 && b == 0 {
                return Err(DecodeError::BadVarint); // overlong: a final zero group adds nothing
            }
            return Ok(v);
        }
    }
    unreachable!("the tenth byte either ends the number or was rejected")
}

/// Reads exactly `len` bytes, where `len` came from untrusted input. Memory grows with the bytes that ARRIVE.
fn read_exact_vec(r: &mut impl Read, len: u64, limit: u64) -> Result<Vec<u8>> {
    if len > limit {
        return Err(DecodeError::TooLarge {
            claimed: len,
            limit,
        });
    }
    let mut v = Vec::new(); // deliberately NOT with_capacity(len)
    r.by_ref().take(len).read_to_end(&mut v)?;
    if v.len() as u64 != len {
        return Err(DecodeError::Truncated);
    }
    Ok(v)
}

// ================================================================ the file format
const MAGIC: [u8; 4] = *b"FFKV";
const VERSION: u16 = 1;
const MAX_RECORD: u64 = 1 << 20;
const TAG_END: u8 = 0;
const TAG_ENTRY: u8 = 1;

fn header_bytes(version: u16, flags: u16) -> [u8; 12] {
    let mut h = [0u8; 12];
    h[..4].copy_from_slice(&MAGIC);
    h[4..6].copy_from_slice(&version.to_le_bytes());
    h[6..8].copy_from_slice(&flags.to_le_bytes());
    let crc = Crc32::of(&h[..8]);
    h[8..].copy_from_slice(&crc.to_le_bytes());
    h
}

fn write_record(w: &mut impl Write, tag: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() as u64 > MAX_RECORD {
        // never write what the reader would refuse
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "record too large",
        ));
    }
    let mut cw = Checked {
        inner: &mut *w,
        crc: Crc32::new(),
    };
    cw.write_all(&[tag])?;
    write_varint(&mut cw, payload.len() as u64)?;
    cw.write_all(payload)?;
    let crc = cw.crc.finish();
    w.write_all(&crc.to_le_bytes())
}

fn encode(map: &HashMap<String, i64>, mut w: impl Write) -> io::Result<()> {
    w.write_all(&header_bytes(VERSION, 0))?;
    // HashMap iteration order differs between maps and runs: sort, so equal maps give identical bytes
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut payload = Vec::new();
    for (k, v) in &entries {
        payload.clear();
        write_varint(&mut payload, k.len() as u64)?; // byte length, not chars().count()
        payload.extend_from_slice(k.as_bytes());
        payload.extend_from_slice(&v.to_le_bytes());
        write_record(&mut w, TAG_ENTRY, &payload)?;
    }
    payload.clear();
    write_varint(&mut payload, entries.len() as u64)?;
    write_record(&mut w, TAG_END, &payload)?;
    w.flush() // report buffered failures to the caller
}

fn read_record(r: &mut impl Read) -> Result<(u8, Vec<u8>)> {
    let mut cr = Checked {
        inner: &mut *r,
        crc: Crc32::new(),
    };
    let [tag] = read_array::<1>(&mut cr)?;
    let len = read_varint(&mut cr)?;
    let payload = read_exact_vec(&mut cr, len, MAX_RECORD)?;
    let computed = cr.crc.finish();
    if u32::from_le_bytes(read_array(r)?) != computed {
        return Err(DecodeError::BadChecksum);
    }
    Ok((tag, payload)) // the tag and payload are interpreted only AFTER the checksum passed
}

fn parse_entry(mut p: &[u8]) -> Result<(String, i64)> {
    // inside a verified payload, running out of bytes is a malformed payload, not a truncated file
    let bad = |_| DecodeError::BadPayload("entry too short");
    let klen = read_varint(&mut p).map_err(|_| DecodeError::BadPayload("bad key length"))?;
    let key = read_exact_vec(&mut p, klen, MAX_RECORD).map_err(bad)?;
    let value = i64::from_le_bytes(read_array(&mut p).map_err(bad)?);
    if !p.is_empty() {
        return Err(DecodeError::BadPayload("extra bytes in entry"));
    }
    let key = String::from_utf8(key).map_err(|_| DecodeError::BadPayload("key is not UTF-8"))?;
    Ok((key, value))
}

/// `read_exact` and `read_to_end` retry `Interrupted` for you; a bare `read` does not, so this loop must.
fn at_eof(r: &mut impl Read) -> io::Result<bool> {
    let mut one = [0u8; 1];
    loop {
        match r.read(&mut one) {
            Ok(n) => return Ok(n == 0),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

fn decode(mut r: impl Read) -> Result<BTreeMap<String, i64>> {
    let h: [u8; 12] = read_array(&mut r)?;
    if h[..4] != MAGIC {
        return Err(DecodeError::BadMagic); // magic first: "not our file" is a better message than "bad checksum"
    }
    if u32::from_le_bytes(h[8..].try_into().expect("4 bytes")) != Crc32::of(&h[..8]) {
        return Err(DecodeError::BadChecksum);
    }
    let version = u16::from_le_bytes([h[4], h[5]]);
    if version != VERSION {
        return Err(DecodeError::UnsupportedVersion(version));
    }
    let flags = u16::from_le_bytes([h[6], h[7]]);
    if flags != 0 {
        return Err(DecodeError::UnknownFlags(flags));
    }
    let mut map = BTreeMap::new();
    let mut previous: Option<String> = None;
    loop {
        let (tag, payload) = read_record(&mut r)?;
        match tag {
            TAG_ENTRY => {
                let (k, v) = parse_entry(&payload)?;
                if previous.as_ref().is_some_and(|p| *p >= k) {
                    return Err(DecodeError::BadPayload("keys not strictly ascending")); // canonical form, no duplicates
                }
                previous = Some(k.clone());
                map.insert(k, v);
            }
            TAG_END => {
                let mut p = payload.as_slice();
                let count =
                    read_varint(&mut p).map_err(|_| DecodeError::BadPayload("bad count"))?;
                if !p.is_empty() || count != map.len() as u64 {
                    return Err(DecodeError::BadPayload("entry count mismatch"));
                }
                return if at_eof(&mut r)? {
                    Ok(map)
                } else {
                    Err(DecodeError::TrailingData)
                };
            }
            other => return Err(DecodeError::UnknownTag(other)),
        }
    }
}

// ================================================================ a text format: tab separated, escaped, one record per line
fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
}
fn unescape(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len()); // sized by data we already hold: safe
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        out.push(match c {
            '\\' => match chars.next()? {
                '\\' => '\\',
                't' => '\t',
                'n' => '\n',
                'r' => '\r',
                _ => return None, // unknown escapes are errors, which leaves room to add new ones
            },
            '\t' | '\n' | '\r' => return None,
            c => c,
        });
    }
    Some(out)
}
fn to_text(rows: &[(String, String)]) -> String {
    let mut out = String::new();
    for (k, v) in rows {
        escape(k, &mut out);
        out.push('\t');
        escape(v, &mut out);
        out.push('\n');
    }
    out
}
fn from_text(s: &str) -> std::result::Result<Vec<(String, String)>, usize> {
    if !s.is_empty() && !s.ends_with('\n') {
        return Err(s.split('\n').count()); // an unterminated last line means truncation
    }
    s.split_terminator('\n')
        .enumerate()
        .map(|(i, line)| {
            line.split_once('\t')
                .and_then(|(k, v)| Some((unescape(k)?, unescape(v)?)))
                .ok_or(i + 1)
        })
        .collect()
}

// ================================================================ tests
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
    fn string(&mut self) -> String {
        let alphabet = [
            'a', 'b', 'Z', '0', ' ', '\t', '\n', '\r', '\\', 't', 'n', 'é', '漢', '🦀', '\0',
        ];
        (0..self.below(12))
            .map(|_| alphabet[self.below(alphabet.len() as u64) as usize])
            .collect()
    }
}

/// Delivers/accepts one byte at a time and interrupts often: a legal but unfriendly stream.
struct Trickle<T> {
    inner: T,
    tick: u32,
}
impl<R: Read> Read for Trickle<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.tick += 1;
        if self.tick % 3 == 0 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = b.len().min(1);
        self.inner.read(&mut b[..n])
    }
}
impl<W: Write> Write for Trickle<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.tick += 1;
        if self.tick % 3 == 0 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.inner.write(&b[..b.len().min(1)])
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn bytes_of(map: &HashMap<String, i64>) -> Vec<u8> {
    let mut v = Vec::new();
    encode(map, &mut v).expect("writing to a Vec");
    v
}

fn s01_primitives() {
    assert_eq!(0x0102_0304u32.to_le_bytes(), [4, 3, 2, 1]);
    assert_eq!(0x0102_0304u32.to_be_bytes(), [1, 2, 3, 4]); // "network byte order"
    assert_eq!(i64::from_be_bytes((-2i64).to_be_bytes()), -2);
    assert!(u32::try_from(u64::from(u32::MAX) + 1).is_err());
    assert_eq!(
        (u64::from(u32::MAX) + 1) as u32,
        0,
        "`as` truncates silently"
    );

    assert_eq!(
        Crc32::of(b"123456789"),
        0xCBF4_3926,
        "the standard CRC-32 check value"
    );
    assert_eq!(Crc32::of(b""), 0);
    let mut c = Crc32::new();
    c.update(b"1234");
    c.update(b"56789");
    assert_eq!(c.finish(), 0xCBF4_3926, "incremental = one shot");

    let enc = |v| {
        let mut b = Vec::new();
        write_varint(&mut b, v).unwrap();
        b
    };
    assert_eq!(enc(0), [0]);
    assert_eq!(enc(127), [127]);
    assert_eq!(enc(128), [0x80, 1]);
    assert_eq!(enc(300), [0xAC, 2]);
    assert_eq!(enc(u64::MAX).len(), 10);
    let mut rng = Rng(1);
    let edges = (0..64).flat_map(|s| [(1u64 << s) - 1, 1u64 << s]);
    for v in edges
        .chain([u64::MAX])
        .chain((0..2_000).map(|_| rng.below(u64::MAX) >> rng.below(64)))
    {
        let b = enc(v);
        let mut r = b.as_slice();
        assert_eq!(read_varint(&mut r).unwrap(), v);
        assert!(r.is_empty(), "consumed exactly its own bytes");
    }
    let bad: [&[u8]; 4] = [
        &[0x80, 0],
        &[0xFF, 0x80, 0],
        &[0xFF; 10],
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 2],
    ];
    for b in bad {
        assert!(
            matches!(read_varint(&mut { b }), Err(DecodeError::BadVarint)),
            "{b:?}"
        );
    }
    assert!(matches!(
        read_varint(&mut [0x80u8].as_slice()),
        Err(DecodeError::Truncated)
    ));
    println!("1. integers / crc32 / varints ok");
}

fn sample() -> HashMap<String, i64> {
    HashMap::from([
        ("alpha".to_string(), 1),
        ("beta".to_string(), -7),
        (String::new(), i64::MIN),
        ("漢字\n\t".to_string(), i64::MAX),
    ])
}

fn s02_roundtrip() {
    let mut rng = Rng(2);
    for case in 0..300 {
        let map: HashMap<String, i64> = (0..rng.below(20))
            .map(|_| (rng.string(), rng.below(u64::MAX) as i64))
            .collect();
        let bytes = bytes_of(&map);
        let back = decode(bytes.as_slice()).unwrap();
        assert!(
            back.len() == map.len() && back.iter().all(|(k, v)| map.get(k) == Some(v)),
            "case {case}"
        );
        // a differently built map (fresh hasher seed, different capacity and insertion order) gives the same bytes
        let mut keys: Vec<_> = map.keys().cloned().collect();
        keys.sort_by(|a, b| b.cmp(a));
        let mut other = HashMap::with_capacity(1000);
        keys.into_iter().for_each(|k| {
            other.insert(k.clone(), map[&k]);
        });
        assert_eq!(bytes_of(&other), bytes);
    }
    // partial reads, partial writes and Interrupted change nothing
    let mut slow = Trickle {
        inner: Vec::new(),
        tick: 0,
    };
    encode(&sample(), &mut slow).unwrap();
    assert_eq!(slow.inner, bytes_of(&sample()));
    assert_eq!(
        decode(Trickle {
            inner: slow.inner.as_slice(),
            tick: 0
        })
        .unwrap()
        .len(),
        4
    );
    // golden bytes: pins the layout, so an accidental format change fails the run
    let golden: &[u8] = &[b'F', b'F', b'K', b'V', 1, 0, 0, 0];
    let one = bytes_of(&HashMap::from([("k".to_string(), 2)]));
    assert_eq!(&one[..8], golden);
    assert_eq!(&one[12..24], &[1, 10, 1, b'k', 2, 0, 0, 0, 0, 0, 0, 0]);
    println!("2. round trips / determinism ok");
}

fn s03_rejections() {
    let good = bytes_of(&sample());
    // EVERY proper prefix fails, and always as Truncated
    for cut in 0..good.len() {
        let r = decode(&good[..cut]);
        assert!(
            matches!(r, Err(DecodeError::Truncated)),
            "cut at {cut}: {r:?}"
        );
    }
    // EVERY single-bit flip in this file is detected
    for byte in 0..good.len() {
        for bit in 0..8 {
            let mut bad = good.clone();
            bad[byte] ^= 1 << bit;
            assert!(
                decode(bad.as_slice()).is_err(),
                "flip {byte}.{bit} accepted"
            );
        }
    }
    let mut extra = good.clone();
    extra.push(0);
    assert!(matches!(
        decode(extra.as_slice()),
        Err(DecodeError::TrailingData)
    ));

    let with_header = |version, flags| header_bytes(version, flags).to_vec();
    assert!(matches!(
        decode(&b"PK\x03\x04 not ours...."[..]),
        Err(DecodeError::BadMagic)
    ));
    assert!(matches!(
        decode(with_header(2, 0).as_slice()),
        Err(DecodeError::UnsupportedVersion(2))
    ));
    assert!(matches!(
        decode(with_header(1, 8).as_slice()),
        Err(DecodeError::UnknownFlags(8))
    ));

    // correctly checksummed but semantically wrong files: the checks behind the CRC
    let file = |records: &[(u8, &[u8])]| {
        let mut f = with_header(1, 0);
        records
            .iter()
            .for_each(|(t, p)| write_record(&mut f, *t, p).unwrap());
        f
    };
    let entry = |k: &[u8]| {
        let mut p = vec![k.len() as u8];
        p.extend_from_slice(k);
        p.extend_from_slice(&0i64.to_le_bytes());
        p
    };
    let (a, b, junk) = (entry(b"a"), entry(b"b"), entry(&[0xFF]));
    assert!(decode(file(&[(1, &a), (1, &b), (0, &[2])]).as_slice()).is_ok());
    type R<'a> = &'a [(u8, &'a [u8])];
    let cases: [R<'_>; 6] = [
        &[(1, &b), (1, &a), (0, &[2])], // wrong order
        &[(1, &a), (1, &a), (0, &[2])], // duplicate
        &[(1, &a), (0, &[2])],          // count mismatch
        &[(1, &junk), (0, &[1])],       // invalid UTF-8
        &[(1, &[5, b'a']), (0, &[1])],  // inner length overruns the payload
        &[(0, &[0, 0])],                // junk after the count
    ];
    for (i, c) in cases.iter().enumerate() {
        assert!(
            matches!(decode(file(c).as_slice()), Err(DecodeError::BadPayload(_))),
            "case {i}"
        );
    }
    assert!(matches!(
        decode(file(&[(9, b"x")]).as_slice()),
        Err(DecodeError::UnknownTag(9))
    ));
    assert!(write_record(&mut Vec::new(), 1, &vec![0; MAX_RECORD as usize + 1]).is_err());

    // length attacks
    let mut attack = with_header(1, 0);
    attack.push(1);
    write_varint(&mut attack, u64::MAX).unwrap();
    assert!(matches!(
        decode(attack.as_slice()),
        Err(DecodeError::TooLarge {
            claimed: u64::MAX,
            limit: MAX_RECORD
        })
    ));
    // within the limit but a lie: the buffer must be sized by what arrived, not by the claim
    let mut v = Vec::new();
    let mut three: &[u8] = b"abc";
    (&mut three).take(MAX_RECORD).read_to_end(&mut v).unwrap();
    assert!(v.capacity() < 64 * 1024, "capacity {}", v.capacity());
    assert!(matches!(
        read_exact_vec(&mut &b"abc"[..], MAX_RECORD, MAX_RECORD),
        Err(DecodeError::Truncated)
    ));

    // real I/O errors are not disguised as bad data, and keep their cause
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "device says no",
            ))
        }
    }
    let e = decode(Broken).unwrap_err();
    assert!(matches!(e, DecodeError::Io(_)));
    assert_eq!(
        std::error::Error::source(&e).unwrap().to_string(),
        "device says no"
    );
    println!(
        "3. rejections ok ({} truncations, {} bit flips)",
        good.len(),
        good.len() * 8
    );
}

fn s04_fuzz() {
    let mut rng = Rng(0xF00D);
    let good = bytes_of(&sample());
    let mut reached_records = 0;
    for case in 0..5_000u32 {
        let mut data: Vec<u8> = (0..rng.below(80)).map(|_| rng.below(256) as u8).collect();
        match case % 3 {
            0 => {} // pure noise (almost always stops at the magic number)
            1 => {
                // a valid header in front, so the noise reaches the record parser
                let mut d = header_bytes(1, 0).to_vec();
                d.append(&mut data);
                data = d;
                reached_records += 1;
            }
            _ => {
                // mutate a valid file: overwrite a few bytes, maybe cut, maybe extend
                let mut d = good.clone();
                for _ in 0..1 + rng.below(4) {
                    let i = rng.below(d.len() as u64) as usize;
                    d[i] = rng.below(256) as u8;
                }
                d.truncate(rng.below(d.len() as u64 + 1) as usize);
                d.extend_from_slice(&data[..data.len().min(4)]);
                data = d;
            }
        }
        // the assertion is "returns": a panic would abort the run. A mutation may by chance restore the original.
        match decode(data.as_slice()) {
            Ok(m) => assert_eq!(
                bytes_of(&m.into_iter().collect()),
                data,
                "case {case}: accepted input must be canonical"
            ),
            Err(e) => assert!(
                !matches!(e, DecodeError::Io(_)),
                "case {case}: slices cannot fail with i/o errors: {e}"
            ),
        }
        // payload parser and varint reader on raw noise, bypassing the checksum that would otherwise shield them
        let _ = parse_entry(&data);
        let _ = read_varint(&mut data.as_slice());
    }
    assert!(reached_records > 1_000);
    println!("4. fuzz ok (5000 inputs, no panic)");
}

fn s05_text() {
    let rows = vec![
        ("a\tb".to_string(), "line1\nline2".to_string()),
        ("\\".to_string(), String::new()),
    ];
    assert_eq!(to_text(&rows), "a\\tb\tline1\\nline2\n\\\\\t\n");
    assert_eq!(from_text(&to_text(&rows)), Ok(rows));
    let mut rng = Rng(5);
    for _ in 0..3_000 {
        let rows: Vec<(String, String)> = (0..rng.below(5))
            .map(|_| (rng.string(), rng.string()))
            .collect();
        let text = to_text(&rows);
        assert_eq!(
            text.matches('\n').count(),
            rows.len(),
            "one physical line per record"
        );
        assert_eq!(from_text(&text), Ok(rows));
        let _ = from_text(&rng.string()); // arbitrary text never panics
    }
    assert_eq!(from_text("a\tb\nno-tab\n"), Err(2));
    assert_eq!(from_text("a\tb\\q\n"), Err(1));
    assert_eq!(from_text("a\tdangling\\\n"), Err(1));
    assert_eq!(from_text("a\tb\nc\td"), Err(2), "truncated final line");
    println!("5. text format ok");
}

fn main() {
    s01_primitives();
    s02_roundtrip();
    s03_rejections();
    s04_fuzz();
    s05_text();
    println!("all serialization sections passed");
}
