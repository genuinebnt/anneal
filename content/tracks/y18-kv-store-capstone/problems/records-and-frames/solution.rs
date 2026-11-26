use thiserror::Error;

#[derive(Debug, Error)]
pub enum KvError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt data: {0}")]
    Corrupt(&'static str),
    #[error("the log is unusable after an earlier failure")]
    Unusable,
}

pub type Result<T> = std::result::Result<T, KvError>;

pub type Key = Vec<u8>;
/// `None` says the key was deleted.
pub type Entry = (Key, Option<Vec<u8>>);

/// CRC-32 as used by zlib, Ethernet and PNG (reflected polynomial 0xEDB88320).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// The next `n` bytes, moving `pos` past them. Every read of bytes from a file goes through here.
fn take<'a>(buf: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = pos
        .checked_add(n)
        .ok_or(KvError::Corrupt("length overflow"))?;
    let bytes = buf.get(*pos..end).ok_or(KvError::Corrupt("short buffer"))?;
    *pos = end;
    Ok(bytes)
}

fn take_u32(buf: &[u8], pos: &mut usize) -> Result<u32> {
    let bytes = take(buf, pos, 4)?;
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| KvError::Corrupt("u32"))?,
    ))
}

pub fn encode_record(key: &[u8], value: Option<&[u8]>, out: &mut Vec<u8>) {
    out.push(u8::from(value.is_some()));
    out.extend((key.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    let value = value.unwrap_or(&[]);
    out.extend((value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
}

pub fn decode_record(buf: &[u8], pos: &mut usize) -> Result<Entry> {
    let mut at = *pos;
    let tag = take(buf, &mut at, 1)?[0];
    let key_len = take_u32(buf, &mut at)? as usize;
    let key = take(buf, &mut at, key_len)?;
    let value_len = take_u32(buf, &mut at)? as usize;
    let value = take(buf, &mut at, value_len)?;
    let value = match tag {
        1 => Some(value.to_vec()),
        0 if value.is_empty() => None,
        _ => return Err(KvError::Corrupt("bad record tag")),
    };
    *pos = at;
    Ok((key.to_vec(), value))
}

pub fn encode_frame(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    let mut record = Vec::new();
    encode_record(key, value, &mut record);
    let mut frame = Vec::with_capacity(8 + record.len());
    frame.extend((record.len() as u32).to_le_bytes());
    frame.extend(crc32(&record).to_le_bytes());
    frame.extend(record);
    frame
}

/// The records of every whole frame at the front of `log`, and the number of bytes those frames take.
pub fn replay(log: &[u8]) -> Result<(Vec<Entry>, usize)> {
    let mut entries = Vec::new();
    let mut valid = 0;
    loop {
        let mut pos = valid;
        let (Ok(len), Ok(crc)) = (take_u32(log, &mut pos), take_u32(log, &mut pos)) else {
            break;
        };
        // no record is empty; eight zero bytes would otherwise pass, because crc32 of nothing is 0
        if len == 0 {
            break;
        }
        let Ok(payload) = take(log, &mut pos, len as usize) else {
            break;
        };
        if crc32(payload) != crc {
            break;
        }
        // the checksum holds, so these are the bytes that were written: if they make no sense, that is not a torn write
        let mut used = 0;
        let entry = decode_record(payload, &mut used)?;
        if used != payload.len() {
            return Err(KvError::Corrupt("bytes after the record"));
        }
        entries.push(entry);
        valid = pos;
    }
    Ok((entries, valid))
}
