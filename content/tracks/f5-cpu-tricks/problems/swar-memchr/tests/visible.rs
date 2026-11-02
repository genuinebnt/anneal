use solution::*;

#[test]
fn splat_repeats_the_byte() {
    check!(r#"0xAB"#, splat(0xAB), 0xABAB_ABAB_ABAB_ABABu64);
}

#[test]
fn one_zero_lane() {
    check!(r#"only lane 4 is zero"#, zero_bytes(0x1122_3300_5566_7788), 0x0000_0080_0000_0000u64);
}

#[test]
fn two_zero_lanes() {
    check!(r#"lanes 0 and 4 are zero"#, zero_bytes(0x1122_3300_5566_7700), 0x0000_0080_0000_0080u64);
}

#[test]
fn find_first() {
    check!(r#"two matches"#, (find_byte(b"hello world, hello", b'o'), find_byte(b"hello", b'z'), find_byte(b"", b'a')), (Some(4), None, None));
}

#[test]
fn find_last() {
    check!(r#"two matches"#, (rfind_byte(b"hello world, hello", b'o'), rfind_byte(b"hello", b'z'), rfind_byte(b"", b'a')), (Some(17), None, None));
}

#[test]
fn count_matches() {
    check!(r#"three matches"#, (count_byte(b"banana", b'a'), count_byte(b"banana", b'n'), count_byte(b"", b'a')), (3, 2, 0));
}
