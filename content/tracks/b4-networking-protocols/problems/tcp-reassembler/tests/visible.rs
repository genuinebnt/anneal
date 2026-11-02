use solution::*;

#[test]
fn in_order() {
    check!(r#"two segments"#, { let mut r = Reassembler::new(1000, 100); r.segment(1000, b"hel", false); let a = r.read(); r.segment(1003, b"lo", false); (a, r.read()) }, (b"hel".to_vec(), b"lo".to_vec()));
}

#[test]
fn out_of_order_waits() {
    check!(r#"the gap is filled later"#, { let mut r = Reassembler::new(0, 100); r.segment(3, b"def", false); let a = r.read(); let held = r.buffered(); r.segment(0, b"abc", false); (a, held, r.read(), r.buffered()) }, (vec![], 3, b"abcdef".to_vec(), 0));
}

#[test]
fn duplicate_is_ignored() {
    check!(r#"same segment twice"#, { let mut r = Reassembler::new(0, 100); r.segment(0, b"abc", false); r.segment(0, b"abc", false); (r.read(), r.buffered()) }, (b"abc".to_vec(), 0));
}

#[test]
fn overlap_extends() {
    check!(r#"a retransmit that carries new bytes"#, { let mut r = Reassembler::new(0, 100); r.segment(0, b"abcd", false); r.segment(2, b"cdef", false); r.read() }, b"abcdef".to_vec());
}

#[test]
fn fin_ends_the_stream() {
    check!(r#"finished only when all bytes arrived"#, { let mut r = Reassembler::new(0, 100); r.segment(3, b"def", true); let early = r.is_finished(); r.segment(0, b"abc", false); (early, r.is_finished(), r.read()) }, (false, true, b"abcdef".to_vec()));
}
