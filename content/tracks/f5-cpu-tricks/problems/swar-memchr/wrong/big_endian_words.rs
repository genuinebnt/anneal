const LO: u64 = 0x0101_0101_0101_0101;
const HI: u64 = 0x8080_8080_8080_8080;

/// `b` copied into all eight bytes.
pub fn splat(b: u8) -> u64 {
    LO * b as u64
}

/// The high bit of each byte of `w` that is zero, nothing else. Exact: no byte's result depends on its neighbours.
pub fn zero_bytes(w: u64) -> u64 {
    !(((w & !HI) + !HI) | w | !HI)
}

fn word(c: &[u8]) -> u64 {
    u64::from_be_bytes(c.try_into().unwrap())
}

pub fn find_byte(hay: &[u8], needle: u8) -> Option<usize> {
    let pat = splat(needle);
    let chunks = hay.chunks_exact(8);
    let rem = chunks.remainder();
    let mut base = 0;
    for c in chunks {
        let m = zero_bytes(word(c) ^ pat);
        if m != 0 {
            return Some(base + (m.trailing_zeros() / 8) as usize);
        }
        base += 8;
    }
    rem.iter().position(|&b| b == needle).map(|i| base + i)
}

pub fn rfind_byte(hay: &[u8], needle: u8) -> Option<usize> {
    let chunks = hay.chunks_exact(8);
    let rem = chunks.remainder();
    let base = hay.len() - rem.len();
    if let Some(i) = rem.iter().rposition(|&b| b == needle) {
        return Some(base + i);
    }
    let pat = splat(needle);
    for (k, c) in chunks.enumerate().rev() {
        let m = zero_bytes(word(c) ^ pat);
        if m != 0 {
            return Some(k * 8 + 7 - (m.leading_zeros() / 8) as usize);
        }
    }
    None
}

pub fn count_byte(hay: &[u8], needle: u8) -> usize {
    let pat = splat(needle);
    let chunks = hay.chunks_exact(8);
    let rem = chunks.remainder();
    let mut n = 0;
    for c in chunks {
        n += zero_bytes(word(c) ^ pat).count_ones() as usize;
    }
    n + rem.iter().filter(|&&b| b == needle).count()
}
