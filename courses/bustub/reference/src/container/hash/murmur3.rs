//! MurmurHash3 for x64 with 128-bit output (Austin Appleby, public domain).

#[inline]
fn fmix64(mut k: u64) -> u64 {
    k ^= k >> 33;
    k = k.wrapping_mul(0xff51afd7ed558ccd);
    k ^= k >> 33;
    k = k.wrapping_mul(0xc4ceb9fe1a85ec53);
    k ^= k >> 33;
    k
}

/// MurmurHash3_x64_128: returns `[h1, h2]`, the two 64-bit halves of the 128-bit hash of `data` with `seed`.
pub fn murmur_hash3_x64_128(data: &[u8], seed: u32) -> [u64; 2] {
    // @begin 2b-01
    const C1: u64 = 0x87c37b91114253d5;
    const C2: u64 = 0x4cf5ad432745937f;
    let len = data.len();
    let mut h1 = seed as u64;
    let mut h2 = seed as u64;

    let mut blocks = data.chunks_exact(16);
    for block in &mut blocks {
        let mut k1 = u64::from_le_bytes(block[..8].try_into().unwrap());
        let mut k2 = u64::from_le_bytes(block[8..].try_into().unwrap());

        k1 = k1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
        h1 ^= k1;
        h1 = h1.rotate_left(27).wrapping_add(h2).wrapping_mul(5).wrapping_add(0x52dce729);

        k2 = k2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
        h2 ^= k2;
        h2 = h2.rotate_left(31).wrapping_add(h1).wrapping_mul(5).wrapping_add(0x38495ab5);
    }

    let tail = blocks.remainder();
    let (mut k1, mut k2) = (0u64, 0u64);
    for (i, &byte) in tail.iter().enumerate().skip(8) {
        k2 ^= (byte as u64) << (8 * (i - 8));
    }
    if tail.len() > 8 {
        k2 = k2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
        h2 ^= k2;
    }
    for (i, &byte) in tail.iter().enumerate().take(8) {
        k1 ^= (byte as u64) << (8 * i);
    }
    if !tail.is_empty() {
        k1 = k1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
        h1 ^= k1;
    }

    h1 ^= len as u64;
    h2 ^= len as u64;
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    h1 = fmix64(h1);
    h2 = fmix64(h2);
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    [h1, h2]
    //~ todo!("2b-01: port MurmurHash3_x64_128: 16-byte blocks, then the tail bytes, then the finalisation (see the stage page)")
    // @end
}
