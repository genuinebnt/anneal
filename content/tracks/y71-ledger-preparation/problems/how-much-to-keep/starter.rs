pub const SLOT: usize = 16;

/// CRC-32, as used by zip and Ethernet.
pub fn checksum(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

/// One slot, as the recorder writes it.
pub fn slot(value: u64, tag: [u8; 4]) -> [u8; SLOT] {
    let mut out = [0; SLOT];
    out[..8].copy_from_slice(&value.to_le_bytes());
    out[8..12].copy_from_slice(&tag);
    let check = checksum(&out[..12]);
    out[12..].copy_from_slice(&check.to_le_bytes());
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tail {
    Clean,
    Torn { at: usize },
    Damaged { at: usize },
}

pub fn inspect(bytes: &[u8]) -> (Vec<u64>, Tail) {
    todo!()
}

/// Cuts a torn end off. `Err(at)` means the file is damaged and has not been touched.
pub fn repair(file: &mut Vec<u8>) -> Result<usize, usize> {
    todo!()
}
