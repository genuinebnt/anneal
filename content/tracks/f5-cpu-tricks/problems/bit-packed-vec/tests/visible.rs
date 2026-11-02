use solution::*;

#[test]
fn small_width_roundtrip() {
    check!(r#"width 3: 0..8"#, { let v = PackedVec::from_slice(3, &[0, 1, 2, 3, 4, 5, 6, 7]).unwrap(); v.iter().collect::<Vec<_>>() }, vec![0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn storage_is_dense() {
    check!(r#"21 values of 3 bits need 63 bits = 1 word, the 22nd needs a second"#, { let mut v = PackedVec::new(3); for _ in 0..21 { v.push(5).unwrap(); } let a = v.words_used(); v.push(5).unwrap(); (a, v.words_used()) }, (1, 2));
}

#[test]
fn too_wide() {
    check!(r#"width 4 refuses 16"#, { let mut v = PackedVec::new(4); (v.push(15), v.push(16), v.len()) }, (Ok(()), Err(PackError::TooWide), 1));
}

#[test]
fn get_out_of_range() {
    check!(r#"empty and past the end"#, { let mut v = PackedVec::new(7); v.push(100).unwrap(); (v.get(0), v.get(1), PackedVec::new(7).get(0)) }, (Some(100), None, None));
}

#[test]
fn set_overwrites_only_its_slot() {
    check!(r#"width 5, set the middle one"#, { let mut v = PackedVec::from_slice(5, &[31, 31, 31]).unwrap(); v.set(1, 0).unwrap(); v.iter().collect::<Vec<_>>() }, vec![31, 0, 31]);
}
