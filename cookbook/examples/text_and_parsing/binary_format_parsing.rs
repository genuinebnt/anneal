//! RECIPE 156 (parsing): safe binary format parsing: a bounds-checked Reader, TLV, a PNG-like chunk container, bit reading, zero-copy.
//! LEVEL: advanced
//! Use in: file formats, network protocols, parsing untrusted input (uploads, packets, plugins) without panics or unsafe.   Run: cargo run --release --example binary_format_parsing
#![allow(dead_code)]
//!
//! THE RULE: input is hostile and may be truncated anywhere. Never index (`buf[i]`) or slice (`buf[a..b]`) raw input; go
//! through a `Reader` whose every primitive checks `n <= remaining()` and returns a typed error carrying the byte OFFSET, so
//! a bug report says "truncated at 0x2c, wanted 4 have 1" instead of a panic. Lengths read from the file are never used to
//! allocate: we return `&[u8]` slices borrowed from the input (zero-copy, lifetime `'a`), so memory is bounded by the input.
//! PIECES: `Reader` (ints LE/BE via a macro, LEB128 varint, length-prefixed bytes, `sub_reader` for nested length-delimited
//! sections so a child can never read past its parent, alignment with zero-padding check); a TLV record decoder that SKIPS
//! unknown tags (forward compatibility, as in protobuf); a PNG-like container `magic | chunk*` with chunk = `len | type |
//! data | crc32(type+data)`, where the type's first-letter case says critical vs ancillary (unknown critical => reject,
//! unknown ancillary => skip); and a MSB-first `BitReader`/`BitWriter` with Exp-Golomb codes (H.264 style variable-width fields).
//! Varints accept overlong encodings (like many decoders); a canonical-only format would also reject those.
//! COMMON USES: image/audio/video container parsers, protobuf-like wire formats, network packet dissectors, database page and
//! WAL formats, archive readers, and sandboxed parsing of user uploads; fuzzing harnesses call exactly these entry points.
//! Read: https://www.w3.org/TR/png/ (chunk layout and critical/ancillary bit), https://protobuf.dev/programming-guides/encoding/
use std::fmt;

// ---------------------------------------------------------------- errors

#[derive(Debug, Clone, PartialEq)]
enum ReadError {
    Eof {
        offset: usize,
        wanted: usize,
        have: usize,
    },
    BitEof {
        bit_offset: usize,
    },
    VarintOverflow {
        offset: usize,
    },
    BadMagic {
        offset: usize,
    },
    BadCrc {
        offset: usize,
        expected: u32,
        actual: u32,
    },
    InvalidValue {
        offset: usize,
        what: &'static str,
    },
    TrailingBytes {
        offset: usize,
    },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ReadError::Eof {
                offset,
                wanted,
                have,
            } => write!(
                f,
                "truncated at {offset:#x}: wanted {wanted} byte(s), have {have}"
            ),
            ReadError::BitEof { bit_offset } => write!(f, "ran out of bits at bit {bit_offset}"),
            ReadError::VarintOverflow { offset } => {
                write!(f, "varint at {offset:#x} overflows u64")
            }
            ReadError::BadMagic { offset } => write!(f, "bad magic at {offset:#x}"),
            ReadError::BadCrc {
                offset,
                expected,
                actual,
            } => write!(
                f,
                "crc mismatch at {offset:#x}: stored {expected:08x}, computed {actual:08x}"
            ),
            ReadError::InvalidValue { offset, what } => {
                write!(f, "invalid value at {offset:#x}: {what}")
            }
            ReadError::TrailingBytes { offset } => {
                write!(f, "unexpected trailing bytes at {offset:#x}")
            }
        }
    }
}

type Res<T> = Result<T, ReadError>;

// ---------------------------------------------------------------- Reader

/// A cursor over borrowed bytes. `base` is the absolute file offset of `buf[0]`, so errors from nested
/// sub-readers still report positions in the ORIGINAL file.
#[derive(Clone, Copy, Debug)]
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    base: usize,
}

macro_rules! int_readers {
    ($($name:ident: $t:ty = $conv:ident;)*) => {
        $(fn $name(&mut self) -> Res<$t> { Ok(<$t>::$conv(self.array()?)) })*
    };
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader {
            buf,
            pos: 0,
            base: 0,
        }
    }
    fn offset(&self) -> usize {
        self.base + self.pos
    }
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    fn peek(&self, n: usize) -> Res<&'a [u8]> {
        if n > self.remaining() {
            return Err(ReadError::Eof {
                offset: self.offset(),
                wanted: n,
                have: self.remaining(),
            });
        }
        Ok(&self.buf[self.pos..self.pos + n]) // cannot overflow: pos + n <= len was just checked
    }
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let s = self.peek(n)?;
        self.pos += n;
        Ok(s)
    }
    fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }
    fn array<const N: usize>(&mut self) -> Res<[u8; N]> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }

    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    int_readers! {
        u16_le: u16 = from_le_bytes; u16_be: u16 = from_be_bytes;
        u32_le: u32 = from_le_bytes; u32_be: u32 = from_be_bytes;
        u64_le: u64 = from_le_bytes; u64_be: u64 = from_be_bytes;
    }

    /// LEB128: 7 bits per byte, low group first. 10 bytes max; the 10th may only carry 1 bit.
    fn varint(&mut self) -> Res<u64> {
        let start = self.offset();
        let mut v = 0u64;
        for i in 0..10 {
            let b = self.u8()?;
            if i == 9 && b > 1 {
                break;
            }
            v |= u64::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(ReadError::VarintOverflow { offset: start })
    }
    /// a length that must fit `usize` (it will normally be checked against `remaining` by `take` right after)
    fn len(&mut self) -> Res<usize> {
        let at = self.offset();
        usize::try_from(self.varint()?).map_err(|_| ReadError::InvalidValue {
            offset: at,
            what: "length too large",
        })
    }
    fn bytes_lp(&mut self) -> Res<&'a [u8]> {
        let n = self.len()?;
        self.take(n)
    }
    /// Carve out the next `n` bytes as an independent reader: the child cannot read past its section.
    fn sub_reader(&mut self, n: usize) -> Res<Reader<'a>> {
        let base = self.offset();
        Ok(Reader {
            buf: self.take(n)?,
            pos: 0,
            base,
        })
    }
    /// Skip padding up to a multiple of `n` (relative to this reader's start). Non-zero padding is rejected:
    /// hidden bytes are a classic smuggling channel and make files non-canonical.
    fn align(&mut self, n: usize) -> Res<()> {
        let pad = (n - self.pos % n) % n;
        let at = self.offset();
        if self.take(pad)?.iter().any(|&b| b != 0) {
            return Err(ReadError::InvalidValue {
                offset: at,
                what: "non-zero padding",
            });
        }
        Ok(())
    }
    fn finish(&self) -> Res<()> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(ReadError::TrailingBytes {
                offset: self.offset(),
            })
        }
    }
}

fn utf8_at(b: &[u8], offset: usize) -> Res<&str> {
    std::str::from_utf8(b).map_err(|e| ReadError::InvalidValue {
        offset: offset + e.valid_up_to(),
        what: "invalid utf-8",
    })
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

// ---------------------------------------------------------------- TLV with unknown-tag skipping

const TAG_NAME: u8 = 1;
const TAG_ID: u8 = 2;
const TAG_SCORE: u8 = 3;
const TAG_LABELS: u8 = 4;

#[derive(Debug, PartialEq)]
struct Record<'a> {
    name: &'a str,
    id: u64,
    scores: Vec<u32>,
    labels: Vec<&'a str>,
    skipped: usize, // unknown tags ignored (not part of the logical value)
}

fn put_tlv(out: &mut Vec<u8>, tag: u8, value: &[u8]) {
    out.push(tag);
    put_varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

fn encode_record(r: &Record, out: &mut Vec<u8>) {
    put_tlv(out, TAG_NAME, r.name.as_bytes());
    let mut id = Vec::new();
    put_varint(&mut id, r.id);
    put_tlv(out, TAG_ID, &id);
    for s in &r.scores {
        put_tlv(out, TAG_SCORE, &s.to_be_bytes());
    }
    let mut labels = Vec::new();
    r.labels
        .iter()
        .for_each(|l| put_tlv(&mut labels, 1, l.as_bytes()));
    put_tlv(out, TAG_LABELS, &labels);
}

fn decode_record<'a>(r: &mut Reader<'a>) -> Res<Record<'a>> {
    let (mut name, mut id, mut scores, mut labels, mut skipped) = (None, 0, vec![], vec![], 0);
    while !r.is_empty() {
        let tag = r.u8()?;
        let len = r.len()?;
        let at = r.offset();
        let mut v = r.sub_reader(len)?; // the value can only see its own bytes
        match tag {
            TAG_NAME => name = Some(utf8_at(v.rest(), at)?),
            TAG_ID => {
                id = v.varint()?;
                v.finish()?;
            }
            TAG_SCORE => {
                scores.push(v.u32_be()?);
                v.finish()?;
            }
            TAG_LABELS => decode_labels(&mut v, &mut labels)?,
            _ => skipped += 1, // unknown tag: its bytes were already consumed by sub_reader
        }
    }
    let name = name.ok_or(ReadError::InvalidValue {
        offset: r.offset(),
        what: "missing name",
    })?;
    Ok(Record {
        name,
        id,
        scores,
        labels,
        skipped,
    })
}

fn decode_labels<'a>(v: &mut Reader<'a>, labels: &mut Vec<&'a str>) -> Res<()> {
    while !v.is_empty() {
        let tag = v.u8()?;
        let len = v.len()?;
        let at = v.offset();
        let body = v.take(len)?;
        if tag == 1 {
            labels.push(utf8_at(body, at)?);
        }
    }
    Ok(())
}

/// Items of `[len u8][bytes][zero pad to 4]`: shows alignment handling.
fn decode_padded_strings<'a>(r: &mut Reader<'a>) -> Res<Vec<&'a str>> {
    let mut out = Vec::new();
    while !r.is_empty() {
        let n = usize::from(r.u8()?);
        let at = r.offset();
        out.push(utf8_at(r.take(n)?, at)?);
        r.align(4)?;
    }
    Ok(out)
}

// ---------------------------------------------------------------- CRC32 and PNG-like container

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let (mut c, mut k) = (i as u32, 0);
        while k < 8 {
            c = if c & 1 != 0 {
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
};

fn crc32_parts(parts: &[&[u8]]) -> u32 {
    let mut c = !0u32;
    for &b in parts.iter().flat_map(|p| p.iter()) {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

const MAGIC: [u8; 8] = *b"\x89FFC\r\n\x1a\n"; // non-ASCII first byte, CRLF and ^Z detect text-mode mangling (like PNG)

#[derive(Debug, PartialEq, Clone, Copy)]
struct Header {
    width: u32,
    height: u32,
    depth: u8,
}

#[derive(Debug, PartialEq)]
struct Container<'a> {
    header: Header,
    data: Vec<&'a [u8]>,
    text: Vec<&'a str>,
    ignored: usize,
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32_parts(&[kind, data]).to_be_bytes());
}

fn encode_container(h: &Header, text: &[&str], data: &[&[u8]]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    let mut head = h.width.to_be_bytes().to_vec();
    head.extend_from_slice(&h.height.to_be_bytes());
    head.push(h.depth);
    write_chunk(&mut out, b"HEAD", &head);
    text.iter()
        .for_each(|t| write_chunk(&mut out, b"tEXt", t.as_bytes()));
    data.iter().for_each(|d| write_chunk(&mut out, b"DATA", d));
    write_chunk(&mut out, b"IEND", &[]);
    out
}

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
    offset: usize,
}

fn read_chunk<'a>(r: &mut Reader<'a>) -> Res<Chunk<'a>> {
    let offset = r.offset();
    let len = r.u32_be()? as usize;
    let kind: [u8; 4] = r.array()?;
    if !kind.iter().all(u8::is_ascii_alphabetic) {
        return Err(ReadError::InvalidValue {
            offset: offset + 4,
            what: "chunk type must be ASCII letters",
        });
    }
    let data = r.take(len)?; // a lying `len` just fails here; nothing is allocated from it
    let crc_at = r.offset();
    let expected = r.u32_be()?;
    let actual = crc32_parts(&[&kind, data]);
    if expected != actual {
        return Err(ReadError::BadCrc {
            offset: crc_at,
            expected,
            actual,
        });
    }
    Ok(Chunk { kind, data, offset })
}

fn parse_header(c: &Chunk) -> Res<Header> {
    let mut r = Reader {
        buf: c.data,
        pos: 0,
        base: c.offset + 8,
    };
    let h = Header {
        width: r.u32_be()?,
        height: r.u32_be()?,
        depth: r.u8()?,
    };
    r.finish()?;
    if !matches!(h.depth, 1 | 2 | 4 | 8 | 16) {
        return Err(ReadError::InvalidValue {
            offset: c.offset + 16,
            what: "bad bit depth",
        });
    }
    Ok(h)
}

fn parse_container(buf: &[u8]) -> Res<Container<'_>> {
    let mut r = Reader::new(buf);
    if r.take(8)? != MAGIC {
        return Err(ReadError::BadMagic { offset: 0 });
    }
    let first = read_chunk(&mut r)?;
    if &first.kind != b"HEAD" {
        return Err(ReadError::InvalidValue {
            offset: first.offset,
            what: "first chunk must be HEAD",
        });
    }
    let mut c = Container {
        header: parse_header(&first)?,
        data: vec![],
        text: vec![],
        ignored: 0,
    };
    loop {
        let ch = read_chunk(&mut r)?; // running out of input before IEND is an error: catches every truncation
        match &ch.kind {
            b"IEND" if !ch.data.is_empty() => {
                return Err(ReadError::InvalidValue {
                    offset: ch.offset,
                    what: "IEND must be empty",
                });
            }
            b"IEND" => {
                r.finish()?;
                return Ok(c);
            }
            b"DATA" => c.data.push(ch.data),
            b"tEXt" => c.text.push(utf8_at(ch.data, ch.offset + 8)?),
            k if k[0].is_ascii_lowercase() => c.ignored += 1, // ancillary: safe to skip
            _ => {
                return Err(ReadError::InvalidValue {
                    offset: ch.offset,
                    what: "unknown critical chunk",
                });
            }
        }
    }
}

// ---------------------------------------------------------------- bit level

/// MSB-first bit reader (JPEG, H.264, MPEG style; deflate uses LSB-first instead).
struct BitReader<'a> {
    buf: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        BitReader { buf, bit: 0 }
    }
    fn bits_left(&self) -> usize {
        self.buf.len() * 8 - self.bit
    }
    fn read_bit(&mut self) -> Res<u8> {
        let byte = *self.buf.get(self.bit / 8).ok_or(ReadError::BitEof {
            bit_offset: self.bit,
        })?;
        let b = (byte >> (7 - self.bit % 8)) & 1;
        self.bit += 1;
        Ok(b)
    }
    fn read_bits(&mut self, n: u32) -> Res<u64> {
        if n > 64 {
            return Err(ReadError::InvalidValue {
                offset: self.bit / 8,
                what: "more than 64 bits requested",
            });
        }
        if n as usize > self.bits_left() {
            return Err(ReadError::BitEof {
                bit_offset: self.bit,
            }); // all-or-nothing: position unchanged
        }
        (0..n).try_fold(0u64, |acc, _| Ok((acc << 1) | u64::from(self.read_bit()?)))
    }
    /// Exp-Golomb ue(v): `z` zeros, a one, then `z` bits. Small values are short; limited to 31 zeros.
    fn ue(&mut self) -> Res<u32> {
        let start = self.bit;
        let mut zeros = 0;
        while self.read_bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return Err(ReadError::InvalidValue {
                    offset: start / 8,
                    what: "exp-golomb too long",
                });
            }
        }
        Ok(((1u64 << zeros) - 1 + self.read_bits(zeros)?) as u32)
    }
    /// signed: 0, 1, -1, 2, -2, ...
    fn se(&mut self) -> Res<i64> {
        let k = i64::from(self.ue()?);
        Ok(if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) })
    }
    /// skip to the next byte boundary; the skipped bits must be zero
    fn align_byte(&mut self) -> Res<()> {
        while self.bit % 8 != 0 {
            if self.read_bit()? != 0 {
                return Err(ReadError::InvalidValue {
                    offset: self.bit / 8,
                    what: "non-zero bit padding",
                });
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    nbits: usize,
}

impl BitWriter {
    fn bit(&mut self, b: u8) {
        if self.nbits % 8 == 0 {
            self.bytes.push(0);
        }
        *self.bytes.last_mut().expect("pushed above") |= b << (7 - self.nbits % 8);
        self.nbits += 1;
    }
    fn bits(&mut self, v: u64, n: u32) {
        (0..n).rev().for_each(|i| self.bit(((v >> i) & 1) as u8));
    }
    fn ue(&mut self, v: u32) {
        let x = u64::from(v) + 1;
        let len = 64 - x.leading_zeros(); // bit length of v+1
        self.bits(0, len - 1);
        self.bits(x, len);
    }
    fn se(&mut self, v: i64) {
        self.ue(if v > 0 {
            (2 * v - 1) as u32
        } else {
            (-2 * v) as u32
        });
    }
}

// ---------------------------------------------------------------- helpers

fn hexdump(data: &[u8]) -> String {
    data.chunks(16)
        .enumerate()
        .map(|(i, row)| {
            let hex: String = (0..16)
                .map(|j| {
                    row.get(j)
                        .map_or("   ".to_string(), |b| format!("{b:02x} "))
                })
                .collect();
            let asc: String = row
                .iter()
                .map(|&b| {
                    if (0x20..0x7f).contains(&b) {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            format!("{:08x}  {hex} |{asc}|", i * 16)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
    fn word(&mut self) -> String {
        (0..self.below(10))
            .map(|_| char::from(b'a' + self.below(26) as u8))
            .collect()
    }
}

fn sample_file(rng: &mut Rng) -> Vec<u8> {
    let h = Header {
        width: rng.next() as u32,
        height: rng.next() as u32,
        depth: [1, 2, 4, 8, 16][rng.below(5)],
    };
    let texts: Vec<String> = (0..rng.below(3)).map(|_| rng.word()).collect();
    let blobs: Vec<Vec<u8>> = (0..rng.below(4))
        .map(|_| {
            let n = rng.below(40);
            rng.bytes(n)
        })
        .collect();
    let (t, b): (Vec<&str>, Vec<&[u8]>) = (
        texts.iter().map(|s| s.as_str()).collect(),
        blobs.iter().map(|v| v.as_slice()).collect(),
    );
    encode_container(&h, &t, &b)
}

// ---------------------------------------------------------------- self-checks

fn test_reader_primitives() {
    let mut r = Reader::new(&[
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0xAC, 0x02, 0x03, b'a', b'b', b'c', 0xFF,
    ]);
    assert_eq!(r.u32_le().unwrap(), 0x0403_0201);
    assert_eq!(r.u32_be().unwrap(), 0x0506_0708);
    assert_eq!(r.varint().unwrap(), 300);
    assert_eq!(r.peek(4).unwrap(), b"\x03abc");
    assert_eq!(r.bytes_lp().unwrap(), b"abc");
    assert_eq!(r.remaining(), 1);
    assert_eq!(
        r.u16_le(),
        Err(ReadError::Eof {
            offset: 14,
            wanted: 2,
            have: 1
        })
    );
    assert_eq!(r.remaining(), 1, "a failed read must not consume");
    // sub-reader errors keep ABSOLUTE offsets and cannot escape their section
    let mut outer = Reader::new(&[9, 9, 1, 2, 3, 4]);
    outer.take(2).unwrap();
    let mut sub = outer.sub_reader(3).unwrap();
    assert_eq!(
        sub.u32_le(),
        Err(ReadError::Eof {
            offset: 2,
            wanted: 4,
            have: 3
        })
    );
    // varint edge cases: max u64 ok, 11 bytes / overflowing 10th byte rejected, truncated rejected
    let mut max = Vec::new();
    put_varint(&mut max, u64::MAX);
    assert_eq!((max.len(), Reader::new(&max).varint()), (10, Ok(u64::MAX)));
    assert!(matches!(
        Reader::new(&[0xff; 10]).varint(),
        Err(ReadError::VarintOverflow { .. })
    ));
    assert!(matches!(
        Reader::new(&[0x80, 0x80]).varint(),
        Err(ReadError::Eof { .. })
    ));
    // a huge declared length must fail cleanly, not allocate or overflow
    let mut huge = Vec::new();
    put_varint(&mut huge, u64::MAX - 1);
    assert!(Reader::new(&huge).bytes_lp().is_err());
    // alignment
    let padded = [1, b'a', 0, 0, 2, b'b', b'c', 0, 3, b'x', 0, 0];
    assert_eq!(
        decode_padded_strings(&mut Reader::new(&padded[..8])).unwrap(),
        ["a", "bc"]
    );
    assert!(decode_padded_strings(&mut Reader::new(&[1, b'a', 0, 7])).is_err());
    assert!(decode_padded_strings(&mut Reader::new(&[1, b'a'])).is_err());
}

fn test_tlv() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..3000 {
        let name = rng.word();
        let labels: Vec<String> = (0..rng.below(4)).map(|_| rng.word()).collect();
        let scores: Vec<u32> = (0..rng.below(4)).map(|_| rng.next() as u32).collect();
        let rec = Record {
            name: &name,
            id: rng.next() >> rng.below(64),
            scores,
            labels: labels.iter().map(String::as_str).collect(),
            skipped: 0,
        };
        let mut enc = Vec::new();
        encode_record(&rec, &mut enc);
        assert_eq!(decode_record(&mut Reader::new(&enc)).unwrap(), rec);
        // forward compatibility: a "newer writer" inserts unknown tags (including a nested blob); we ignore them
        let mut newer = Vec::new();
        put_tlv(&mut newer, 200, &rng.bytes(5));
        newer.extend_from_slice(&enc);
        put_tlv(&mut newer, 77, &[]);
        let got = decode_record(&mut Reader::new(&newer)).unwrap();
        assert_eq!((got.skipped, Record { skipped: 0, ..got }), (2, rec));
        // every truncation is an error or a shorter-but-valid prefix of records; never a panic
        for cut in 0..enc.len() {
            let _ = decode_record(&mut Reader::new(&enc[..cut]));
        }
    }
    // a tag whose length runs past the end is reported with its offset
    assert_eq!(
        decode_record(&mut Reader::new(&[TAG_NAME, 50, b'x'])),
        Err(ReadError::Eof {
            offset: 2,
            wanted: 50,
            have: 1
        })
    );
}

fn test_container() {
    let mut rng = Rng(0xC0FF_EE00_DEAD_BEEF);
    for _ in 0..500 {
        let file = sample_file(&mut rng);
        let c = parse_container(&file).unwrap();
        // zero-copy: every returned slice points INTO `file`
        let range = file.as_ptr_range();
        assert!(
            c.data
                .iter()
                .all(|d| d.is_empty() || range.contains(&d.as_ptr()))
        );
        // EVERY truncation errors
        for cut in 0..file.len() {
            assert!(
                parse_container(&file[..cut]).is_err(),
                "truncated to {cut} of {} parsed",
                file.len()
            );
        }
        // trailing garbage is rejected too
        assert!(matches!(
            parse_container(&[file.clone(), vec![0]].concat()),
            Err(ReadError::TrailingBytes { .. })
        ));
    }
    // forward compat: unknown ancillary chunk skipped, unknown critical chunk rejected
    let h = Header {
        width: 2,
        height: 3,
        depth: 8,
    };
    let base = encode_container(&h, &["hi"], &[b"xyz"]);
    let body_end = base.len() - 12; // IEND chunk is 12 bytes
    for (kind, ok) in [(b"zzzz", true), (b"ZZZZ", false)] {
        let mut f = base[..body_end].to_vec();
        write_chunk(&mut f, kind, b"future");
        write_chunk(&mut f, b"IEND", &[]);
        assert_eq!(parse_container(&f).is_ok(), ok);
    }
    // all single-bit flips of a small file: never a panic, and CRC/magic/length checks catch every one
    let file = encode_container(&h, &["hello"], &[b"payload-bytes"]);
    let mut rejected = 0;
    for i in 0..file.len() * 8 {
        let mut m = file.clone();
        m[i / 8] ^= 1 << (i % 8);
        if parse_container(&m).is_err() {
            rejected += 1;
        } else {
            assert_ne!(m, file); // accepted only if semantically harmless (none expected)
        }
    }
    println!(
        "bit flips: {rejected}/{} single-bit mutations rejected",
        file.len() * 8
    );
    assert_eq!(rejected, file.len() * 8);
    // random multi-byte mutations: no panic
    for _ in 0..5000 {
        let mut m = file.clone();
        (0..1 + rng.below(4)).for_each(|_| {
            let at = rng.below(m.len());
            m[at] = rng.next() as u8;
        });
        let _ = parse_container(&m);
    }
    assert_eq!(
        crc32_parts(&[b"1234", b"56789"]),
        0xCBF4_3926,
        "standard CRC-32 check value"
    );
}

fn test_bits() {
    let mut w = BitWriter::default();
    w.bits(0b101, 3);
    for v in [0, 1, 2, 7, 1000, u32::MAX - 1] {
        w.ue(v);
    }
    for v in [0, 1, -1, 5, -77] {
        w.se(v);
    }
    w.bits(0xDEAD_BEEF_CAFE, 48);
    let mut r = BitReader::new(&w.bytes);
    assert_eq!(r.read_bits(3).unwrap(), 0b101);
    for v in [0, 1, 2, 7, 1000, u32::MAX - 1] {
        assert_eq!(r.ue().unwrap(), v);
    }
    for v in [0, 1, -1, 5, -77] {
        assert_eq!(r.se().unwrap(), v);
    }
    assert_eq!(r.read_bits(48).unwrap(), 0xDEAD_BEEF_CAFE);
    assert!(r.align_byte().is_ok());
    assert_eq!(
        r.read_bit(),
        Err(ReadError::BitEof {
            bit_offset: w.bytes.len() * 8
        })
    );
    assert!(
        BitReader::new(&[0, 0, 0, 0, 0]).ue().is_err(),
        "all-zero input must not loop or overflow"
    );
    assert!(BitReader::new(&[0b1000_0001]).read_bits(1).is_ok());
    let mut r = BitReader::new(&[0b1000_0001]);
    r.read_bits(1).unwrap();
    assert!(r.align_byte().is_err(), "padding bits must be zero");
}

fn test_fuzz() {
    let mut rng = Rng(0xF00D_F00D_1234_4321);
    let (mut deep, mut parsed) = (0, 0);
    for round in 0..20_000 {
        let n = rng.below(120);
        let mut buf = rng.bytes(n);
        if round % 4 == 0 {
            buf.splice(0..0, MAGIC); // reach the chunk parser instead of failing on the magic
        }
        deep += usize::from(parse_container(&buf).is_err());
        parsed += usize::from(decode_record(&mut Reader::new(&buf)).is_ok());
        let _ = decode_padded_strings(&mut Reader::new(&buf));
        let mut bits = BitReader::new(&buf);
        while bits.ue().is_ok() || bits.se().is_ok() {
            if bits.bits_left() == 0 {
                break;
            }
        }
    }
    println!("fuzz: 20000 random inputs, 0 panics ({parsed} happened to decode as a TLV record)");
    assert!(deep > 0);
}

fn main() {
    let h = Header {
        width: 640,
        height: 480,
        depth: 8,
    };
    let file = encode_container(&h, &["made by r156"], &[b"pixels!", b"more pixels"]);
    println!(
        "--- container ({} bytes) ---\n{}",
        file.len(),
        hexdump(&file)
    );
    let c = parse_container(&file).unwrap();
    println!("parsed: {c:?}");
    assert_eq!(
        (c.header, c.data.len(), c.text.clone()),
        (h, 2, vec!["made by r156"])
    );

    let mut bad = file.clone();
    bad[46] ^= 0x40;
    println!("corrupt byte 46  => {}", parse_container(&bad).unwrap_err());
    println!(
        "truncate to 40   => {}",
        parse_container(&file[..40]).unwrap_err()
    );
    println!(
        "wrong magic      => {}",
        parse_container(b"GIF89a..........").unwrap_err()
    );

    let mut tlv = Vec::new();
    encode_record(
        &Record {
            name: "ada",
            id: 300,
            scores: vec![7, 9],
            labels: vec!["x", "yz"],
            skipped: 0,
        },
        &mut tlv,
    );
    put_tlv(&mut tlv, 99, b"from the future");
    println!(
        "--- TLV ---\n{}\n{:?}",
        hexdump(&tlv),
        decode_record(&mut Reader::new(&tlv)).unwrap()
    );

    test_reader_primitives();
    test_tlv();
    test_container();
    test_bits();
    test_fuzz();
    println!("all binary-format checks passed");
}
