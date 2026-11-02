use solution::*;

#[test]
fn layout_follows_c_rules() {
    check!(r#"12 bytes, align 4, padded"#, layout(), (12, 4, [0, 4, 8]));
}

#[test]
fn to_bytes_has_zero_padding() {
    check!(r#"tag, 3 pad, len, flags, 2 pad"#, to_bytes(&Header { tag: 7, len: 0x01020304, flags: 0x0506 }) == { let mut b = [0u8; 12]; b[0] = 7; b[4..8].copy_from_slice(&0x01020304u32.to_ne_bytes()); b[8..10].copy_from_slice(&0x0506u16.to_ne_bytes()); b }, true);
}

#[test]
fn from_bytes_round_trip() {
    check!(r#"write then read"#, { let h = Header { tag: 255, len: u32::MAX, flags: 1 }; from_bytes(&to_bytes(&h)) }, Some(Header { tag: 255, len: u32::MAX, flags: 1 }));
}

#[test]
fn short_input_is_none() {
    check!(r#"11 bytes"#, (from_bytes(&[0u8; 11]), view(&[0u8; 11]).is_none()), (None, true));
}

#[test]
fn from_bytes_ignores_padding_contents() {
    check!(r#"garbage in the padding"#, { let mut b = to_bytes(&Header { tag: 1, len: 2, flags: 3 }); b[1] = 0xAA; b[11] = 0xBB; from_bytes(&b) }, Some(Header { tag: 1, len: 2, flags: 3 }));
}
