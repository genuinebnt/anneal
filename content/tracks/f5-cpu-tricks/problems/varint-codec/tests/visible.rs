use solution::*;

#[test]
fn small_values_take_one_byte() {
    check!(r#"0, 1, 127"#, { let mut b = Vec::new(); for v in [0u64, 1, 127] { encode_u64(v, &mut b); } b }, vec![0, 1, 127]);
}

#[test]
fn multi_byte() {
    check!(r#"128, 300"#, { let mut b = Vec::new(); encode_u64(128, &mut b); encode_u64(300, &mut b); b }, vec![0x80, 0x01, 0xac, 0x02]);
}

#[test]
fn decode_reports_length() {
    check!(r#"300 followed by junk"#, decode_u64(&[0xac, 0x02, 0xff]), Ok((300, 2)));
}

#[test]
fn zigzag_order() {
    check!(r#"0, -1, 1, -2, 2"#, [0i64, -1, 1, -2, 2].map(zigzag), [0, 1, 2, 3, 4]);
}

#[test]
fn sorted_roundtrip() {
    check!(r#"3, 3, 10, 500"#, { let mut b = Vec::new(); encode_sorted(&[3, 3, 10, 500], &mut b).unwrap(); (b.clone(), decode_sorted(&b)) }, (vec![4, 3, 0, 7, 0xea, 0x03], Ok(vec![3, 3, 10, 500])));
}
