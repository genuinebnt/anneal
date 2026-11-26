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
    todo!()
}

pub fn encode_record(key: &[u8], value: Option<&[u8]>, out: &mut Vec<u8>) {
    todo!()
}

pub fn decode_record(buf: &[u8], pos: &mut usize) -> Result<Entry> {
    todo!()
}

pub fn encode_frame(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    todo!()
}

/// The records of every whole frame at the front of `log`, and the number of bytes those frames take.
pub fn replay(log: &[u8]) -> Result<(Vec<Entry>, usize)> {
    todo!()
}
