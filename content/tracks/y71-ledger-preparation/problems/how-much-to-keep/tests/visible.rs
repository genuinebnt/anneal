use solution::*;

#[allow(dead_code)]
fn file_of(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|&v| slot(v, *b"alt ")).collect()
}

#[test]
fn a_whole_file() {
    let f = file_of(&[10, 20, 30]);
    check!("three slots", inspect(&f), (vec![10, 20, 30], Tail::Clean));
    check!("an empty file", inspect(&[]), (vec![], Tail::Clean));
}

#[test]
fn a_piece_at_the_end() {
    let mut f = file_of(&[10, 20]);
    f.extend_from_slice(&slot(30, *b"alt ")[..9]);
    check!("9 bytes of a third slot", inspect(&f), (vec![10, 20], Tail::Torn { at: 32 }));
    check!("repair", repair(&mut f), Ok(9));
    check!("after", f, file_of(&[10, 20]));
}

#[test]
fn zeros_at_the_end() {
    let mut f = file_of(&[10]);
    f.resize(16 + 50, 0);
    check!("50 zeros", inspect(&f), (vec![10], Tail::Torn { at: 16 }));
    check!("repair", repair(&mut f), Ok(50));
}

#[test]
fn damage_before_good_data_is_not_cut_off() {
    let mut f = file_of(&[10, 20, 30]);
    f[17] ^= 4;
    let before = f.clone();
    check!("second slot damaged", inspect(&f), (vec![10], Tail::Damaged { at: 16 }));
    check!("repair", repair(&mut f), Err(16));
    check!("untouched", f, before);
}

#[test]
fn the_last_slot_half_written() {
    let mut f = file_of(&[10, 20]);
    for b in &mut f[24..32] {
        *b = 0;
    }
    check!("second half missing", inspect(&f), (vec![10], Tail::Torn { at: 16 }));
}
