//! Errors, the checksum and the record layout. Every read of bytes that came from disk goes through `take`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum KvError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt data: {0}")]
    Corrupt(&'static str),
    #[error("the log is unusable after an earlier failure")]
    LogUnusable,
}
pub type Result<T> = std::result::Result<T, KvError>;

pub type Key = Vec<u8>;
pub type Val = Option<Vec<u8>>; // None = tombstone
pub type Entry = (Key, Val);

/// CRC-32 (the reflected 0xEDB88320 polynomial), one bit at a time: short, not fast.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

// ---------- bounds-checked decoding ----------
pub(crate) fn take<'a>(buf: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = pos.checked_add(n).ok_or(KvError::Corrupt("length overflow"))?;
    let s = buf.get(*pos..end).ok_or(KvError::Corrupt("short buffer"))?;
    *pos = end;
    Ok(s)
}
pub(crate) fn take_u32(buf: &[u8], pos: &mut usize) -> Result<u32> {
    let b = take(buf, pos, 4)?.try_into().map_err(|_| KvError::Corrupt("u32"))?;
    Ok(u32::from_le_bytes(b))
}
pub(crate) fn take_u64(buf: &[u8], pos: &mut usize) -> Result<u64> {
    let b = take(buf, pos, 8)?.try_into().map_err(|_| KvError::Corrupt("u64"))?;
    Ok(u64::from_le_bytes(b))
}

/// One record layout shared by the WAL payload and the SSTable data block: [tag][klen][key][vlen][value]
pub fn encode_entry(key: &[u8], val: Option<&[u8]>, out: &mut Vec<u8>) {
    out.push(val.is_some() as u8);
    out.extend((key.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    let v = val.unwrap_or(&[]);
    out.extend((v.len() as u32).to_le_bytes());
    out.extend_from_slice(v);
}

pub fn decode_entry(buf: &[u8], pos: &mut usize) -> Result<Entry> {
    let tag = take(buf, pos, 1)?[0];
    let klen = take_u32(buf, pos)? as usize;
    let key = take(buf, pos, klen)?.to_vec();
    let vlen = take_u32(buf, pos)? as usize;
    let val = take(buf, pos, vlen)?.to_vec();
    match tag {
        1 => Ok((key, Some(val))),
        0 => Ok((key, None)),
        _ => Err(KvError::Corrupt("bad record tag")),
    }
}
