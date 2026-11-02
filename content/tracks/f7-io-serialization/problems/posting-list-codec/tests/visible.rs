use solution::*;

#[test]
fn a_tiny_list() {
    check!(r#"1, 2, 3 are deltas 1, 1, 1 at one bit each"#, encode(&[1, 2, 3]), Ok(vec![3, 1, 0b111]));
}

#[test]
fn a_wider_delta() {
    check!(r#"5 then a gap of 295 needs 9 bits"#, encode(&[5, 300]), Ok(vec![2, 9, 5, 0x4E, 2]));
}

#[test]
fn the_empty_list() {
    check!(r#"just the count"#, (encode(&[]), decode(&[0])), (Ok(vec![0]), Ok(vec![])));
}

#[test]
fn zero_width_blocks() {
    check!(r#"all deltas zero: no payload"#, (encode(&[0, 0, 0]), decode(&[3, 0])), (Ok(vec![3, 0]), Ok(vec![0, 0, 0])));
}

#[test]
fn round_trip() {
    check!(r#"a few hundred ids"#, { let ids: Vec<u32> = (0..300u32).map(|i| i * i / 7 + i).collect(); decode(&encode(&ids).unwrap()) == Ok(ids) }, true);
}

#[test]
fn unsorted_input() {
    check!(r#"an error"#, encode(&[3, 2]), Err(EncodeError::NotSorted));
}
