//! RECIPE 105 (ffi): repr(transparent/u8/C/packed), offset_of!, extern "C" fn pointers, CStr, and a binary round trip.
//! LEVEL: intermediate
//! Use in: FFI boundaries, binary file/wire formats, mmap'd structures.   Run: cargo run --release --example repr_layout_ffi
#![allow(dead_code)]
//!
//! WHAT: default `repr(Rust)` layout is unspecified. To match C or a byte format you opt in explicitly:
//!   - `repr(transparent)`: a one-field newtype has exactly the ABI/layout of the field (safe to pass where the inner type goes).
//!   - `repr(u8)` on a fieldless enum: fixed size and explicit discriminants. Turn bytes into it with `TryFrom<u8>`, NOT `transmute`
//!     (an out-of-range byte transmuted into an enum is instant UB).
//!   - `repr(C)`: fields in declaration order with C padding rules. `repr(C, packed)`: no padding, so fields may be unaligned;
//!     never take `&field` of a packed struct, copy the value out.
//! HOW: parse bytes by hand (`from_le_bytes`, portable and safe) and, as a cross-check, with `ptr::read_unaligned` (one unsafe
//! line, valid because every bit pattern is a valid `Header`). Decoders must return errors on bad input, never panic.
//! `extern "C" fn` items are plain Rust functions with the C calling convention; no linking is needed to call them via pointer.
//! COMMON USES: wrapping C library handles, reading on-disk headers and network packet formats, passing callbacks to C APIs,
//!   `CString`/`CStr` at every C-string boundary.
//! Read: https://doc.rust-lang.org/reference/type-layout.html and https://doc.rust-lang.org/nomicon/other-reprs.html
use std::ffi::{CStr, CString};
use std::mem::{align_of, offset_of, size_of};
use std::ptr;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct Meters(f64);

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Sample = 1,
    Event = 2,
    Marker = 0x10,
}
impl TryFrom<u8> for Kind {
    type Error = DecodeError;
    fn try_from(b: u8) -> Result<Self, DecodeError> {
        match b {
            1 => Ok(Kind::Sample),
            2 => Ok(Kind::Event),
            0x10 => Ok(Kind::Marker),
            other => Err(DecodeError::BadKind(other)),
        }
    }
}

#[derive(Debug, PartialEq)]
enum DecodeError {
    Truncated { need: usize, have: usize },
    BadMagic(u32),
    BadKind(u8),
}

const MAGIC: u32 = 0x4657_4642; // "BFWF" little-endian bytes
/// Raw on-wire header; `kind` stays a u8 here so every bit pattern is valid.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct RawHeader {
    magic: u32,
    version: u16,
    kind: u8,
    flags: u8,
    count: u32,
}
const HEADER_LEN: usize = 12;
const _: () = assert!(size_of::<RawHeader>() == HEADER_LEN && align_of::<RawHeader>() == 4);
const _: () = assert!(offset_of!(RawHeader, version) == 4 && offset_of!(RawHeader, count) == 8);

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct FileHeader {
    tag: u8,
    len: u32,
    crc: u16,
}
const _: () = assert!(size_of::<FileHeader>() == 7 && align_of::<FileHeader>() == 1);
const _: () = assert!(offset_of!(FileHeader, len) == 1 && offset_of!(FileHeader, crc) == 5);

#[derive(Debug, PartialEq)]
struct Record {
    id: u32,
    value: i32,
}

fn parse_header_manual(b: &[u8]) -> Result<RawHeader, DecodeError> {
    if b.len() < HEADER_LEN {
        return Err(DecodeError::Truncated {
            need: HEADER_LEN,
            have: b.len(),
        });
    }
    Ok(RawHeader {
        magic: u32::from_le_bytes(b[0..4].try_into().unwrap()),
        version: u16::from_le_bytes(b[4..6].try_into().unwrap()),
        kind: b[6],
        flags: b[7],
        count: u32::from_le_bytes(b[8..12].try_into().unwrap()),
    })
}
fn parse_header_unsafe(b: &[u8]) -> Result<RawHeader, DecodeError> {
    if b.len() < HEADER_LEN {
        return Err(DecodeError::Truncated {
            need: HEADER_LEN,
            have: b.len(),
        });
    }
    // SAFETY: length checked above; RawHeader is plain integers with no padding, so any bytes are valid;
    // read_unaligned imposes no alignment requirement on the source pointer.
    let h = unsafe { ptr::read_unaligned(b.as_ptr() as *const RawHeader) };
    // Fields are native-endian; convert from little-endian (a no-op on LE hosts).
    Ok(RawHeader {
        magic: u32::from_le(h.magic),
        version: u16::from_le(h.version),
        count: u32::from_le(h.count),
        ..h
    })
}

fn encode(kind: Kind, records: &[Record]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + records.len() * 8);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.push(kind as u8);
    out.push(0);
    out.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for r in records {
        out.extend_from_slice(&r.id.to_le_bytes());
        out.extend_from_slice(&r.value.to_le_bytes());
    }
    out
}
fn decode(b: &[u8]) -> Result<(Kind, Vec<Record>), DecodeError> {
    let h = parse_header_manual(b)?;
    if h.magic != MAGIC {
        return Err(DecodeError::BadMagic(h.magic));
    }
    let kind = Kind::try_from(h.kind)?;
    let need = (h.count as usize)
        .checked_mul(8)
        .and_then(|n| n.checked_add(HEADER_LEN))
        .unwrap_or(usize::MAX);
    if b.len() < need {
        return Err(DecodeError::Truncated {
            need,
            have: b.len(),
        });
    }
    let recs = b[HEADER_LEN..need]
        .chunks_exact(8)
        .map(|c| Record {
            id: u32::from_le_bytes(c[0..4].try_into().unwrap()),
            value: i32::from_le_bytes(c[4..8].try_into().unwrap()),
        })
        .collect();
    Ok((kind, recs))
}

extern "C" fn c_add(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

fn main() {
    // transparent
    assert_eq!(size_of::<Meters>(), size_of::<f64>());
    assert_eq!(align_of::<Meters>(), align_of::<f64>());
    // repr(u8) enum
    assert_eq!(size_of::<Kind>(), 1);
    assert_eq!(Kind::Marker as u8, 0x10);
    assert_eq!(Kind::try_from(2), Ok(Kind::Event));
    assert_eq!(Kind::try_from(3), Err(DecodeError::BadKind(3)));
    // let k: Kind = unsafe { std::mem::transmute(3u8) }; // UB: 3 is not a valid discriminant. Use TryFrom.

    let recs = vec![
        Record { id: 1, value: -5 },
        Record {
            id: 2,
            value: 70000,
        },
    ];
    let bytes = encode(Kind::Sample, &recs);
    assert_eq!(bytes.len(), HEADER_LEN + 16);
    let (k, back) = decode(&bytes).unwrap();
    assert_eq!((k, back), (Kind::Sample, recs));
    let (m, u) = (
        parse_header_manual(&bytes).unwrap(),
        parse_header_unsafe(&bytes).unwrap(),
    );
    assert_eq!(m, u);
    assert_eq!(m.count, 2);
    // misaligned source slice still works
    let mut shifted = vec![0u8];
    shifted.extend_from_slice(&bytes);
    assert_eq!(parse_header_unsafe(&shifted[1..]).unwrap(), m);

    // corruption returns errors
    assert_eq!(
        decode(&bytes[..5]),
        Err(DecodeError::Truncated { need: 12, have: 5 })
    );
    assert!(matches!(
        decode(&bytes[..bytes.len() - 1]),
        Err(DecodeError::Truncated { .. })
    ));
    let mut bad = bytes.clone();
    bad[0] ^= 0xFF;
    assert!(matches!(decode(&bad), Err(DecodeError::BadMagic(_))));
    let mut bad = bytes.clone();
    bad[6] = 99;
    assert_eq!(decode(&bad), Err(DecodeError::BadKind(99)));
    let mut bad = bytes.clone();
    bad[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(decode(&bad), Err(DecodeError::Truncated { .. })));

    // packed: copy fields out, never reference them
    let fh = FileHeader {
        tag: 7,
        len: 0xDEAD_BEEF,
        crc: 0x1234,
    };
    let (len, crc) = (fh.len, fh.crc); // by-value copies are fine
    // let r = &fh.len; // ERROR (E0793): reference to packed field is unaligned
    assert_eq!((fh.tag, len, crc), (7, 0xDEAD_BEEF, 0x1234));

    // extern "C" via function pointer
    let f: extern "C" fn(i32, i32) -> i32 = c_add;
    assert_eq!(f(2, 3), 5);

    // CString / CStr round trip
    let cs = CString::new("hello").unwrap();
    assert_eq!(cs.as_bytes_with_nul(), b"hello\0");
    let view: &CStr = CStr::from_bytes_with_nul(cs.as_bytes_with_nul()).unwrap();
    assert_eq!(view.to_str().unwrap(), "hello");
    assert!(CString::new("a\0b").is_err()); // interior NUL rejected
    assert!(CStr::from_bytes_with_nul(b"no nul").is_err());
    println!("header {:?}, round trip ok", m);
}
