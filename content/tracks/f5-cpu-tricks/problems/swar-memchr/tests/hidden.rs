use solution::*;

#[test]
fn exact_above_a_zero() {
    check!(r#"lane 0 is zero, lane 1 is 0x01 (a borrow would flag it)"#, zero_bytes(0x1111_1111_1111_0100), 0x0000_0000_0000_0080u64);
}

#[test]
fn match_in_last_byte_of_a_word() {
    check!(r#"index 7"#, find_byte(b"abcdefgXijklmnop", b'X'), Some(7));
}

#[test]
fn match_in_first_byte_of_a_word() {
    check!(r#"index 8"#, find_byte(b"abcdefghXjklmnop", b'X'), Some(8));
}

#[test]
fn match_in_the_tail() {
    check!(r#"index 18 of 19"#, find_byte(b"abcdefghijklmnopqrX", b'X'), Some(18));
}

#[test]
fn zero_needle() {
    check!(r#"NUL bytes"#, (find_byte(&[1, 2, 3, 4, 5, 6, 7, 8, 0, 9], 0), rfind_byte(&[0, 2, 3, 4, 5, 6, 7, 8, 0, 9], 0), count_byte(&[0; 20], 0)), (Some(8), Some(8), 20));
}

#[test]
fn high_bytes() {
    check!(r#"0xFF and 0x80"#, (find_byte(&[0x80, 0xFF, 1, 2, 3, 4, 5, 6, 0xFF], 0xFF), rfind_byte(&[0x80, 0xFF, 1, 2, 3, 4, 5, 6, 0xFF], 0xFF), count_byte(&[0xFF; 17], 0xFF)), (Some(1), Some(8), 17));
}

#[test]
fn count_is_exact_with_ones_above_zeros() {
    check!(r#"needle 0 over bytes 0,1,0,1..."#, count_byte(&[0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1], 0), 8);
}

#[test]
fn rfind_in_a_full_word() {
    check!(r#"last of several in one word"#, rfind_byte(b"aXbXcXdXeeeeeeee", b'X'), Some(7));
}

#[test]
fn random_vs_naive_every_offset() {
    let mut rng = anneal_prelude::Rng::new(6401);
    let buf: Vec<u8> = (0..600).map(|_| (rng.below(6) as u8) * 0x2B).collect(); // few distinct values, includes 0
    for start in 0..9 {
        for len in 0..70 {
            let s = &buf[start..start + len];
            for needle in [0u8, 0x2B, 0x56, 0xD7, 0x01] {
                let f = s.iter().position(|&b| b == needle);
                let r = s.iter().rposition(|&b| b == needle);
                let c = s.iter().filter(|&&b| b == needle).count();
                check!("find/rfind/count agree with a byte loop", (find_byte(s, needle), rfind_byte(s, needle), count_byte(s, needle)), (f, r, c));
            }
        }
    }
}

#[test]
fn zero_bytes_random_words() {
    let mut rng = anneal_prelude::Rng::new(6402);
    for _ in 0..20_000 {
        let mut w = 0u64;
        for lane in 0..8 {
            let b = match rng.below(4) { 0 => 0, 1 => 1, 2 => 0x80, _ => rng.below(256) as u8 };
            w |= (b as u64) << (8 * lane);
        }
        let mut want = 0u64;
        for lane in 0..8 {
            if (w >> (8 * lane)) & 0xFF == 0 {
                want |= 0x80 << (8 * lane);
            }
        }
        check!("zero_bytes is exact in every lane", zero_bytes(w), want);
    }
}
