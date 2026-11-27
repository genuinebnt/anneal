/// The most bytes one block can hold.
const BLOCK: usize = 65535;

/// The Adler-32 checksum.
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// `data` as a zlib stream that compresses nothing.
pub fn wrap(data: &[u8]) -> Vec<u8> {
    // Empty data still needs one block, the one that says it is the last.
    let blocks = data.len().div_ceil(BLOCK).max(1);
    let mut out = Vec::with_capacity(data.len() + 5 * blocks + 6);
    out.extend_from_slice(&[0x78, 0x01]);
    for i in 0..blocks {
        let piece = &data[i * BLOCK..data.len().min((i + 1) * BLOCK)];
        out.push(u8::from(i + 1 == blocks));
        let length = piece.len() as u16;
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&(!length).to_le_bytes());
        out.extend_from_slice(piece);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// The data of a zlib stream made of stored blocks, or `None` if it is not exactly that.
pub fn unwrap(stream: &[u8]) -> Option<Vec<u8>> {
    let (&[first, second], mut rest) = stream.split_first_chunk::<2>()?;
    let sound = first & 0x0F == 8
        && first >> 4 <= 7
        && (u32::from(first) * 256 + u32::from(second)) % 31 == 0;
    if !sound || second & 0x20 != 0 {
        return None;
    }
    let mut data = Vec::new();
    loop {
        let (&[kind, a, b, c, d], after) = rest.split_first_chunk::<5>()?;
        let length = u16::from_le_bytes([a, b]);
        if kind > 1 || u16::from_le_bytes([c, d]) != !length {
            return None;
        }
        let (piece, after) = after.split_at(usize::from(length));
        data.extend_from_slice(piece);
        rest = after;
        if kind == 1 {
            break;
        }
    }
    let sum: [u8; 4] = rest.try_into().ok()?;
    (u32::from_be_bytes(sum) == adler32(&data)).then_some(data)
}
