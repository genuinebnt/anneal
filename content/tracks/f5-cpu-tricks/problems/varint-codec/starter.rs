#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DecodeError {
    /// The input ended inside a number.
    Truncated,
    /// The number does not fit in 64 bits.
    Overflow,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct NotSorted {
    pub index: usize,
}

/// LEB128: 7 bits per byte, least significant group first, the high bit says "more follows".
pub fn encode_u64(v: u64, out: &mut Vec<u8>) {
    todo!()
}

/// The value and the number of bytes it took. Accepts non-minimal encodings (`[0x80, 0x00]` is 0, 2 bytes), rejects
/// anything that does not fit in 64 bits (`Overflow`) and input that ends mid-number (`Truncated`).
pub fn decode_u64(buf: &[u8]) -> Result<(u64, usize), DecodeError> {
    todo!()
}

/// Maps small magnitudes to small codes: 0, -1, 1, -2, 2 become 0, 1, 2, 3, 4 (works on `i64::MIN` too).
pub fn zigzag(v: i64) -> u64 {
    todo!()
}

pub fn unzigzag(u: u64) -> i64 {
    todo!()
}

pub fn encode_i64(v: i64, out: &mut Vec<u8>) {
    todo!()
}

pub fn decode_i64(buf: &[u8]) -> Result<(i64, usize), DecodeError> {
    todo!()
}

/// Layout: the count, then the first value, then each difference to the previous value, all as varints.
/// Equal neighbours are fine; a decrease is `Err(NotSorted { index })` with the index of the element that is too small.
/// On error nothing may be appended to `out`.
pub fn encode_sorted(values: &[u64], out: &mut Vec<u8>) -> Result<(), NotSorted> {
    todo!()
}

/// The inverse of `encode_sorted`; bytes after the list are ignored. Do not trust the count: a handful of bytes can
/// claim 2^60 elements, and that must be a `Truncated` error, not an allocation. A sum past `u64::MAX` is `Overflow`.
pub fn decode_sorted(buf: &[u8]) -> Result<Vec<u64>, DecodeError> {
    todo!()
}
