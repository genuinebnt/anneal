/// `b` copied into all eight bytes of a u64.
pub fn splat(b: u8) -> u64 {
    todo!()
}

/// The high bit (0x80) of each byte of `w` that is zero, and no other bit set. Must be exact for every byte, with no
/// false positives from borrows between neighbouring bytes.
pub fn zero_bytes(w: u64) -> u64 {
    todo!()
}

/// Index of the first `needle`, scanning eight bytes per step.
pub fn find_byte(hay: &[u8], needle: u8) -> Option<usize> {
    todo!()
}

/// Index of the last `needle`.
pub fn rfind_byte(hay: &[u8], needle: u8) -> Option<usize> {
    todo!()
}

/// How many bytes equal `needle`.
pub fn count_byte(hay: &[u8], needle: u8) -> usize {
    todo!()
}
