use solution::*;

#[test]
fn u64_max_is_ten_bytes() {
    check!(r#"u64::MAX"#, { let mut b = Vec::new(); encode_u64(u64::MAX, &mut b); (b.clone(), decode_u64(&b)) }, (vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01], Ok((u64::MAX, 10))));
}

#[test]
fn overflow_in_the_tenth_byte() {
    check!(r#"10th byte 0x02 would be bit 64"#, decode_u64(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02]), Err(DecodeError::Overflow));
}

#[test]
fn eleven_bytes_overflow() {
    check!(r#"continuation after the 10th byte"#, decode_u64(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01]), Err(DecodeError::Overflow));
}

#[test]
fn truncated() {
    check!(r#"continuation bit but no more bytes; empty input"#, (decode_u64(&[0x80]), decode_u64(&[0xff, 0xff]), decode_u64(&[])), (Err(DecodeError::Truncated), Err(DecodeError::Truncated), Err(DecodeError::Truncated)));
}

#[test]
fn non_minimal_is_accepted() {
    check!(r#"[0x80, 0x00] is 0 in two bytes"#, (decode_u64(&[0x80, 0x00]), decode_u64(&[0xe5, 0x8e, 0x26])), (Ok((0, 2)), Ok((624485, 3))));
}

#[test]
fn zigzag_extremes() {
    check!(r#"i64::MIN and i64::MAX"#, (zigzag(i64::MIN), zigzag(i64::MAX), unzigzag(u64::MAX), unzigzag(u64::MAX - 1), unzigzag(zigzag(i64::MIN))), (u64::MAX, u64::MAX - 1, i64::MIN, i64::MAX, i64::MIN));
}

#[test]
fn signed_roundtrip_sizes() {
    check!(r#"-1 is one byte, i64::MIN is ten"#, { let mut a = Vec::new(); encode_i64(-1, &mut a); let mut b = Vec::new(); encode_i64(i64::MIN, &mut b); (a.len(), b.len(), decode_i64(&a), decode_i64(&b)) }, (1, 10, Ok((-1, 1)), Ok((i64::MIN, 10))));
}

#[test]
fn sorted_rejects_a_decrease() {
    check!(r#"5, 9, 8"#, { let mut b = vec![42]; let r = encode_sorted(&[5, 9, 8], &mut b); (r, b) }, (Err(NotSorted { index: 2 }), vec![42]));
}

#[test]
fn sorted_empty_and_single() {
    check!(r#"[] and [7]"#, { let mut a = Vec::new(); encode_sorted(&[], &mut a).unwrap(); let mut b = Vec::new(); encode_sorted(&[7], &mut b).unwrap(); (a.clone(), decode_sorted(&a), b.clone(), decode_sorted(&b)) }, (vec![0], Ok(vec![]), vec![1, 7], Ok(vec![7])));
}

#[test]
fn sorted_decode_ignores_trailing_bytes() {
    check!(r#"list then junk"#, decode_sorted(&[2, 1, 1, 0xde, 0xad]), Ok(vec![1, 2]));
}

#[test]
fn lying_count_does_not_allocate() {
    check!(r#"count 2^60 with three bytes of data"#, { let mut b = Vec::new(); encode_u64(1 << 60, &mut b); b.extend([1, 1, 1]); decode_sorted(&b) }, Err(DecodeError::Truncated));
}

#[test]
fn delta_sum_overflow() {
    check!(r#"deltas pass u64::MAX"#, { let mut b = vec![2]; encode_u64(u64::MAX, &mut b); encode_u64(5, &mut b); decode_sorted(&b) }, Err(DecodeError::Overflow));
}

#[test]
fn random_roundtrips() {
    let mut rng = anneal_prelude::Rng::new(6303);
    for _ in 0..400 {
        let bits = rng.below(65) as u32;
        let v = if bits == 0 { 0 } else { (((rng.below(1 << 31) as u64) << 33) | rng.below(1 << 31) as u64 * 4) >> (64 - bits) };
        let mut b = Vec::new();
        encode_u64(v, &mut b);
        let expect_len = ((64 - v.leading_zeros()).max(1) as usize).div_ceil(7);
        check!(format!("u64 {v}"), (decode_u64(&b), b.len()), (Ok((v, expect_len)), expect_len));
        let s = v as i64;
        let mut c = Vec::new();
        encode_i64(s, &mut c);
        check!(format!("i64 {s}"), decode_i64(&c), Ok((s, c.len())));
    }
}

#[test]
fn random_sorted_lists() {
    let mut rng = anneal_prelude::Rng::new(6304);
    for _ in 0..300 {
        let n = rng.below(20);
        let mut v: Vec<u64> = (0..n).map(|_| rng.below(1_000_000) as u64 * rng.below(1000) as u64).collect();
        v.sort();
        let mut b = Vec::new();
        encode_sorted(&v, &mut b).unwrap();
        check!(format!("sorted list {v:?}"), decode_sorted(&b), Ok(v.clone()));
        if b.len() > 1 {
            let cut = rng.below(b.len() - 1) + 1;
            let r = decode_sorted(&b[..cut]);
            assert!(r.is_err() || r.as_ref().map(|x| x.len()).unwrap_or(0) < v.len() || cut == b.len(), "a truncated buffer decoded to {r:?}");
        }
    }
}

#[test]
fn garbage_never_panics() {
    let mut rng = anneal_prelude::Rng::new(6305);
    for _ in 0..2000 {
        let n = rng.below(16);
        let bytes: Vec<u8> = (0..n).map(|_| rng.below(256) as u8).collect();
        let _ = decode_u64(&bytes);
        let _ = decode_i64(&bytes);
        let _ = decode_sorted(&bytes);
    }
}

#[test]
fn a_hundred_thousand_ids() {
    let mut acc = 0u64;
    let ids: Vec<u64> = (0..100_000u64).map(|i| { acc += 1 + i % 7; acc }).collect();
    let mut b = Vec::new();
    encode_sorted(&ids, &mut b).unwrap();
    check!("100000 increasing ids: decoded equal, and compact (about 1 byte each)", (decode_sorted(&b).unwrap() == ids, b.len() < 110_000), (true, true));
}
