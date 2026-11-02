#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// The ids are not in non-decreasing order.
    NotSorted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The input ends before the structure it describes does.
    Truncated,
    /// A block declares more than 32 bits per delta.
    BadWidth(u8),
    /// A running sum above `u32::MAX`.
    Overflow,
    /// Bytes left over after the last block.
    TrailingBytes,
}

/// Ids per block (the last block may hold fewer).
pub const BLOCK: usize = 128;

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn get_varint(bytes: &[u8], at: &mut usize) -> Result<u64, DecodeError> {
    let mut v = 0u64;
    let mut shift = 0;
    loop {
        let b = *bytes.get(*at).ok_or(DecodeError::Truncated)?;
        *at += 1;
        v |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
        if shift > 35 {
            return Err(DecodeError::Overflow);
        }
    }
}

pub fn encode(ids: &[u32]) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    put_varint(&mut out, ids.len() as u64);
    let mut prev = 0u32;
    for block in ids.chunks(BLOCK) {
        let mut deltas = Vec::with_capacity(block.len());
        for &id in block {
            if id < prev {
                return Err(EncodeError::NotSorted);
            }
            deltas.push(id - prev);
            prev = id;
        }
        let max = deltas.iter().copied().max().unwrap_or(0);
        let w = (32 - max.leading_zeros()) as usize + usize::from(max > 0 && max.is_power_of_two());
        out.push(w as u8);
        let mut acc: u64 = 0;
        let mut nbits = 0usize;
        for d in deltas {
            acc |= (d as u64) << nbits;
            nbits += w;
            while nbits >= 8 {
                out.push(acc as u8);
                acc >>= 8;
                nbits -= 8;
            }
        }
        if nbits > 0 {
            out.push(acc as u8); // the padded last byte
        }
    }
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<Vec<u32>, DecodeError> {
    let mut at = 0;
    let count = get_varint(bytes, &mut at)? as usize;
    let mut ids = Vec::new();
    let mut prev: u64 = 0;
    let mut remaining = count;
    while remaining > 0 {
        let n = remaining.min(BLOCK);
        let w = *bytes.get(at).ok_or(DecodeError::Truncated)?;
        at += 1;
        if w > 32 {
            return Err(DecodeError::BadWidth(w));
        }
        let w = w as usize;
        let nbytes = (n * w + 7) / 8;
        let packed = bytes.get(at..at + nbytes).ok_or(DecodeError::Truncated)?;
        at += nbytes;
        let mut acc: u64 = 0;
        let mut nbits = 0usize;
        let mut next = 0usize;
        for _ in 0..n {
            while nbits < w {
                acc |= (packed[next] as u64) << nbits;
                next += 1;
                nbits += 8;
            }
            let d = if w == 0 { 0 } else { acc & ((1u64 << w) - 1) };
            acc >>= w;
            nbits -= w;
            prev += d;
            if prev > u32::MAX as u64 {
                return Err(DecodeError::Overflow);
            }
            ids.push(prev as u32);
        }
        remaining -= n;
    }
    if at != bytes.len() {
        return Err(DecodeError::TrailingBytes);
    }
    Ok(ids)
}
