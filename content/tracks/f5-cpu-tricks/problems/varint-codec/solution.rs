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
pub fn encode_u64(mut v: u64, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// The value and the number of bytes it took. Non-minimal encodings (such as `[0x80, 0x00]`) are accepted, as protobuf does.
pub fn decode_u64(buf: &[u8]) -> Result<(u64, usize), DecodeError> {
    let mut v = 0u64;
    for (i, &b) in buf.iter().enumerate() {
        let shift = 7 * i as u32;
        if shift == 63 && b > 1 {
            return Err(DecodeError::Overflow); // the 10th byte may only contribute the top bit
        }
        if shift > 63 {
            return Err(DecodeError::Overflow);
        }
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok((v, i + 1));
        }
    }
    Err(DecodeError::Truncated)
}

/// Maps small magnitudes to small codes: 0, -1, 1, -2, 2 become 0, 1, 2, 3, 4.
pub fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

pub fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

pub fn encode_i64(v: i64, out: &mut Vec<u8>) {
    encode_u64(zigzag(v), out)
}

pub fn decode_i64(buf: &[u8]) -> Result<(i64, usize), DecodeError> {
    let (u, n) = decode_u64(buf)?;
    Ok((unzigzag(u), n))
}

/// Layout: count, then the first value, then each difference to the previous one, all varints.
/// Equal neighbours are fine (difference 0); a decrease is `Err(NotSorted { index })` of the offending element.
pub fn encode_sorted(values: &[u64], out: &mut Vec<u8>) -> Result<(), NotSorted> {
    if let Some(i) = values.windows(2).position(|w| w[1] < w[0]) {
        return Err(NotSorted { index: i + 1 });
    }
    encode_u64(values.len() as u64, out);
    let mut prev = 0u64;
    for &v in values {
        encode_u64(v - prev, out);
        prev = v;
    }
    Ok(())
}

/// The inverse. The buffer must contain exactly one encoded list (trailing bytes are ignored). Must not trust the count:
/// a few bytes can claim 2^60 elements.
pub fn decode_sorted(buf: &[u8]) -> Result<Vec<u64>, DecodeError> {
    let (count, mut pos) = decode_u64(buf)?;
    let mut out = Vec::with_capacity((count as usize).min(buf.len()));
    let mut prev = 0u64;
    for _ in 0..count {
        let (d, n) = decode_u64(&buf[pos..])?;
        prev = prev.checked_add(d).ok_or(DecodeError::Overflow)?;
        out.push(prev);
        pos += n;
    }
    Ok(out)
}
