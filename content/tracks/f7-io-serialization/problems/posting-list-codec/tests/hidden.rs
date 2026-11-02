use solution::*;

#[test]
fn block_boundaries_carry_the_previous_id() {
    check!(r#"130 consecutive ids: two blocks, one bit each"#, { let ids: Vec<u32> = (1..=130).collect(); let b = encode(&ids).unwrap(); (b[..2].to_vec(), b[2], b[3..19].to_vec(), b[19], b[20], b.len()) }, (vec![0x82, 0x01], 1, vec![0xFF; 16], 1, 3, 21));
}

#[test]
fn duplicates_are_allowed() {
    check!(r#"delta 0 inside a list"#, { let ids = vec![4u32, 4, 4, 9, 9]; decode(&encode(&ids).unwrap()) == Ok(ids) }, true);
}

#[test]
fn extremes_use_all_32_bits() {
    check!(r#"0 then u32::MAX"#, { let ids = vec![0u32, u32::MAX]; let b = encode(&ids).unwrap(); (b[1], decode(&b) == Ok(ids)) }, (32, true));
}

#[test]
fn exact_block_sizes() {
    check!(r#"127, 128, 129 ids"#, [127usize, 128, 129, 256, 257].iter().all(|&n| { let ids: Vec<u32> = (0..n as u32).map(|i| i * 3).collect(); decode(&encode(&ids).unwrap()) == Ok(ids) }), true);
}

#[test]
fn dense_lists_compress_well() {
    check!(r#"1000 consecutive ids"#, { let ids: Vec<u32> = (0..1000).collect(); let n = encode(&ids).unwrap().len(); n < 160 }, true);
}

#[test]
fn each_block_picks_its_own_width() {
    check!(r#"a sparse block then a dense one"#, { let mut ids: Vec<u32> = (0..128u32).map(|i| i * 1000).collect(); let last = *ids.last().unwrap(); ids.extend((1..=128u32).map(|i| last + i)); let b = encode(&ids).unwrap(); let w1 = b[2]; let w2 = b[2 + 1 + 128 * w1 as usize / 8 + usize::from(128 * w1 as usize % 8 != 0)]; (w1, w2) }, (10, 1));
}

#[test]
fn truncated_inputs() {
    check!(r#"every prefix of a valid encoding fails"#, { let ids: Vec<u32> = (0..200u32).map(|i| i * 17).collect(); let b = encode(&ids).unwrap(); (0..b.len()).all(|k| decode(&b[..k]).is_err()) }, true);
}

#[test]
fn trailing_bytes() {
    check!(r#"one extra byte"#, { let mut b = encode(&[1, 2, 3]).unwrap(); b.push(0); decode(&b) }, Err(DecodeError::TrailingBytes));
}

#[test]
fn bad_width() {
    check!(r#"w = 33"#, decode(&[1, 33, 0, 0, 0, 0, 0]), Err(DecodeError::BadWidth(33)));
}

#[test]
fn overflowing_sum() {
    check!(r#"deltas add up past u32::MAX"#, decode(&[2, 32, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]), Err(DecodeError::Overflow));
}

#[test]
fn random_sorted_lists_round_trip() {
    let mut rng = anneal_prelude::Rng::new(1234);
    for round in 0..300 {
        let n = rng.below(700);
        let mut cur = 0u64;
        let mut ids = Vec::with_capacity(n);
        let spread = 1u64 << rng.below(20);
        for _ in 0..n {
            cur += rng.below(spread as usize + 1) as u64;
            if cur > u32::MAX as u64 {
                break;
            }
            ids.push(cur as u32);
        }
        let bytes = encode(&ids).unwrap();
        let count = ids.len();
        check!(format!("round {round}: n = {count}"), decode(&bytes), Ok(ids));
    }
}
