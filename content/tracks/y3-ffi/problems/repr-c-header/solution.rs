use std::mem::{align_of, offset_of, size_of};

/// The C struct `struct header { uint8_t tag; uint32_t len; uint16_t flags; };`
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub tag: u8,
    pub len: u32,
    pub flags: u16,
}

pub fn layout() -> (usize, usize, [usize; 3]) {
    (size_of::<Header>(), align_of::<Header>(), [offset_of!(Header, tag), offset_of!(Header, len), offset_of!(Header, flags)])
}

pub fn to_bytes(h: &Header) -> [u8; 12] {
    let mut out = [0u8; 12]; // zeroed first: the padding bytes stay 0
    out[offset_of!(Header, tag)] = h.tag;
    let l = offset_of!(Header, len);
    out[l..l + 4].copy_from_slice(&h.len.to_ne_bytes());
    let f = offset_of!(Header, flags);
    out[f..f + 2].copy_from_slice(&h.flags.to_ne_bytes());
    out
}

pub fn from_bytes(b: &[u8]) -> Option<Header> {
    if b.len() < size_of::<Header>() {
        return None;
    }
    // every bit pattern is a valid u8/u32/u16, so a byte-wise read is sound whatever the alignment
    Some(unsafe { std::ptr::read_unaligned(b.as_ptr() as *const Header) })
}

pub fn view(b: &[u8]) -> Option<&Header> {
    if b.len() < size_of::<Header>() || (b.as_ptr() as usize) % align_of::<Header>() != 0 {
        return None;
    }
    Some(unsafe { &*(b.as_ptr() as *const Header) })
}
