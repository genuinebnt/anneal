use std::mem::{align_of, offset_of, size_of};

/// The C struct `struct header { uint8_t tag; uint32_t len; uint16_t flags; };`
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub tag: u8,
    pub len: u32,
    pub flags: u16,
}

/// `(size, alignment, [offset of tag, offset of len, offset of flags])` of `Header` under the C layout rules.
pub fn layout() -> (usize, usize, [usize; 3]) {
    todo!()
}

/// The 12 bytes C would have in memory for `h` (native endianness). Padding bytes are zero, never uninitialised.
pub fn to_bytes(h: &Header) -> [u8; 12] {
    todo!()
}

/// Reads a `Header` from bytes in the C layout. `None` if fewer than 12 bytes are given. Works for any alignment.
pub fn from_bytes(b: &[u8]) -> Option<Header> {
    todo!()
}

/// Reinterprets the bytes in place as a `&Header` without copying. `None` unless there are at least 12 bytes
/// **and** the address is correctly aligned for `Header`.
pub fn view(b: &[u8]) -> Option<&Header> {
    todo!()
}
