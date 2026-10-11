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
    todo!("2b-01: port MurmurHash3_x64_128: 16-byte blocks, then the tail bytes, then the finalisation (see the stage page)")
}
