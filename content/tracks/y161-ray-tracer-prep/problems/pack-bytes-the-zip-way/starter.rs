/// The Adler-32 checksum.
pub fn adler32(data: &[u8]) -> u32 {
    todo!()
}

/// `data` as a zlib stream that compresses nothing.
pub fn wrap(data: &[u8]) -> Vec<u8> {
    todo!()
}

/// The data of a zlib stream made of stored blocks, or `None` if it is not exactly that.
pub fn unwrap(stream: &[u8]) -> Option<Vec<u8>> {
    todo!()
}
