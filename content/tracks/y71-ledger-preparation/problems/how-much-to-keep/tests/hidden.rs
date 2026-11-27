use solution::*;

#[allow(dead_code)]
fn file_of(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|&v| slot(v, *b"alt ")).collect()
}

#[test]
fn sixteen_zeros_are_not_a_slot() {
    check!("one zero slot", inspect(&[0; 16]), (vec![], Tail::Torn { at: 0 }));
    let mut f = vec![0; 16];
    f.extend(file_of(&[5]));
    check!("zeros, then a good slot", inspect(&f), (vec![], Tail::Damaged { at: 0 }));
}

#[test]
fn a_value_of_zero_is_fine() {
    let f: Vec<u8> = (0..3).flat_map(|_| slot(0, [0; 4])).collect();
    check!("three zero readings", inspect(&f), (vec![0, 0, 0], Tail::Clean));
}

#[test]
fn a_damaged_slot_followed_only_by_zeros() {
    let mut f = file_of(&[1, 2]);
    f[16] ^= 1;
    f.resize(100, 0);
    check!("torn", inspect(&f), (vec![1], Tail::Torn { at: 16 }));
}

#[test]
fn a_single_byte_after_the_damaged_slot() {
    let mut f = file_of(&[1, 2]);
    f[16] ^= 1;
    f.push(7);
    check!("one byte more", inspect(&f), (vec![1], Tail::Damaged { at: 16 }));
}

#[test]
fn junk_hidden_behind_zeros_counts() {
    let mut f = file_of(&[1]);
    f.resize(16 + 4096, 0);
    f.push(1);
    check!("a byte after many zeros", inspect(&f), (vec![1], Tail::Damaged { at: 16 }));
    let before = f.clone();
    check!("repair", repair(&mut f), Err(16));
    check!("untouched", f, before);
}

#[test]
fn zeros_in_the_middle_are_damage() {
    let mut f = file_of(&[1, 2, 3]);
    for b in &mut f[16..32] {
        *b = 0;
    }
    check!("a lost middle slot", inspect(&f), (vec![1], Tail::Damaged { at: 16 }));
}

#[test]
fn the_check_covers_the_tag() {
    let mut f = file_of(&[1, 2]);
    f[8] ^= 1;
    check!("tag changed", inspect(&f), (vec![], Tail::Damaged { at: 0 }));
}

#[test]
fn repairing_twice_changes_nothing_more() {
    let mut f = file_of(&[1]);
    f.push(9);
    check!("first", repair(&mut f), Ok(1));
    check!("second", repair(&mut f), Ok(0));
    check!("file", f, file_of(&[1]));
}

#[test]
fn every_crash_keeps_whole_slots() {
    let mut rng = anneal_prelude::Rng::new(7108);
    for _ in 0..150 {
        let n = rng.below(6);
        let values: Vec<u64> = (0..n).map(|_| rng.next_u64()).collect();
        let whole = file_of(&values);
        for cut in 0..=whole.len() {
            let kept = cut / 16;
            let tail = if cut % 16 == 0 { Tail::Clean } else { Tail::Torn { at: kept * 16 } };
            check!(format!("{n} slots cut at {cut}"), inspect(&whole[..cut]), (values[..kept].to_vec(), tail));
            // the file grew but the data past the cut never arrived
            let zeros = rng.below(40);
            let mut grown = whole[..cut].to_vec();
            grown.resize(cut + zeros, 0);
            let good = (0..grown.len() / 16).take_while(|&i| i < n && grown[i * 16..i * 16 + 16] == whole[i * 16..i * 16 + 16]).count();
            let want = if grown.len() == good * 16 { Tail::Clean } else { Tail::Torn { at: good * 16 } };
            check!(format!("{n} slots cut at {cut}, then {zeros} zeros"), inspect(&grown), (values[..good].to_vec(), want));
        }
    }
}

#[test]
fn every_flipped_bit_before_the_last_slot_is_damage() {
    let values = [u64::MAX, 0, 77];
    let whole = file_of(&values);
    for bit in 0..32 * 8 {
        let mut f = whole.clone();
        f[bit / 8] ^= 1 << (bit % 8);
        let slot = bit / 128;
        check!(format!("bit {bit}"), inspect(&f), (values[..slot].to_vec(), Tail::Damaged { at: slot * 16 }));
        let before = f.clone();
        check!(format!("bit {bit}"), repair(&mut f), Err(slot * 16));
        check!(format!("bit {bit}"), f, before);
    }
}

#[test]
fn a_large_file() {
    let values: Vec<u64> = (0..2_000).collect();
    let mut f = file_of(&values);
    f.resize(f.len() + (1 << 20), 0);
    check!("repair", repair(&mut f), Ok(1 << 20));
    check!("kept", inspect(&f).0.len(), 2_000);
}
