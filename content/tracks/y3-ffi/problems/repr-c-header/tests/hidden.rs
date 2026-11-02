use solution::*;

#[repr(align(4))]
struct Aligned([u8; 20]);

fn bytes(tag: u8, len: u32, flags: u16) -> [u8; 12] {
    let mut b = [0u8; 12];
    b[0] = tag;
    b[4..8].copy_from_slice(&len.to_ne_bytes());
    b[8..10].copy_from_slice(&flags.to_ne_bytes());
    b
}

#[test]
fn view_needs_alignment() {
    let buf = Aligned([0u8; 20]);
    check!("aligned address", view(&buf.0[..12]).is_some(), true);
    check!("offset by one", view(&buf.0[1..13]).is_none(), true);
    check!("offset by two", view(&buf.0[2..14]).is_none(), true);
    check!("offset by four is aligned again", view(&buf.0[4..16]).is_some(), true);
}

#[test]
fn view_reads_in_place() {
    let mut buf = Aligned([0u8; 20]);
    buf.0[..12].copy_from_slice(&bytes(9, 1000, 77));
    let h = view(&buf.0[..12]).unwrap();
    check!("fields", (h.tag, h.len, h.flags), (9, 1000, 77));
    check!("it is the same memory, not a copy", h as *const Header as usize, buf.0.as_ptr() as usize);
}

#[test]
fn from_bytes_works_at_every_alignment() {
    let buf = Aligned([0u8; 20]);
    let mut buf = buf;
    buf.0[1..13].copy_from_slice(&bytes(5, 123456, 321));
    check!("misaligned source", from_bytes(&buf.0[1..13]), Some(Header { tag: 5, len: 123456, flags: 321 }));
    buf.0[3..15].copy_from_slice(&bytes(6, 7, 8));
    check!("offset 3", from_bytes(&buf.0[3..15]), Some(Header { tag: 6, len: 7, flags: 8 }));
}

#[test]
fn extra_trailing_bytes_are_ignored() {
    let mut v = bytes(1, 2, 3).to_vec();
    v.extend_from_slice(&[9, 9, 9, 9]);
    check!("16 bytes given", from_bytes(&v), Some(Header { tag: 1, len: 2, flags: 3 }));
    check!("exactly 12 is enough", from_bytes(&v[..12]).is_some(), true);
    check!("11 is not", from_bytes(&v[..11]), None);
}

#[test]
fn to_bytes_is_deterministic_and_matches_the_layout() {
    let h = Header { tag: 0xFF, len: 0, flags: 0xFFFF };
    let b = to_bytes(&h);
    check!("tag at offset 0", b[0], 0xFF);
    check!("padding after the tag", &b[1..4], &[0u8, 0, 0][..]);
    check!("flags at offset 8", &b[8..10], &0xFFFFu16.to_ne_bytes()[..]);
    check!("padding at the end", &b[10..12], &[0u8, 0][..]);
    check!("same input, same bytes", to_bytes(&h), b);
}

#[test]
fn layout_matches_size_of() {
    check!(r#"the numbers are the compiler's"#, (layout().0 == std::mem::size_of::<Header>(), layout().1 == std::mem::align_of::<Header>()), (true, true));
}

#[test]
fn size_is_a_multiple_of_alignment() {
    check!(r#"arrays of Header stay aligned"#, layout().0 % layout().1, 0);
}

#[test]
fn sum_of_fields_is_not_the_size() {
    check!(r#"1 + 4 + 2 = 7 is wrong"#, layout().0 > 1 + 4 + 2, true);
}
