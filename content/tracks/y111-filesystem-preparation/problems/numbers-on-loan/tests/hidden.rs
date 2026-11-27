use solution::*;
use std::collections::BTreeSet;

#[test]
fn nothing_to_lend() {
    for (count, reserved) in [(0, 0), (0, 5), (4, 4), (4, u32::MAX)] {
        let mut lender = Lender::new(count, reserved);
        check!(format!("count {count}, reserved {reserved}"), (lender.free(), lender.take(), lender.bytes().len()), (0, None, count.div_ceil(8) as usize));
    }
}

#[test]
fn a_map_that_is_too_short_is_refused() {
    check!("17 bits in 2 bytes", Lender::from_bytes(&[0, 0], 17, 0).is_none(), true);
    check!("16 bits in 2 bytes", Lender::from_bytes(&[0, 0], 16, 0).is_some(), true);
    // refused without trying to make a map of that size
    check!("u32::MAX bits in 3 bytes", Lender::from_bytes(&[0, 0, 0], u32::MAX, 0).is_none(), true);
    check!("nothing for nothing", Lender::from_bytes(&[], 0, 0).map(|l| l.free()), Some(0));
}

#[test]
fn bytes_and_bits_past_the_end_mean_nothing() {
    let mut lender = Lender::from_bytes(&[0b1111_1000, 0xAA, 0xBB], 3, 0).unwrap();
    check!("free", lender.free(), 3);
    check!("only one byte is kept, with the extra bits cleared", lender.bytes().to_vec(), vec![0]);
    let got: Vec<Option<u32>> = (0..4).map(|_| lender.take()).collect();
    check!("takes", got, vec![Some(0), Some(1), Some(2), None]);
    check!("bytes", lender.bytes().to_vec(), vec![0b111]);
}

#[test]
fn reserved_numbers_stay_taken_whatever_the_map_says() {
    let lender = Lender::from_bytes(&[0, 0], 16, 10).unwrap();
    check!("free", lender.free(), 6);
    check!("bytes", lender.bytes().to_vec(), vec![0xFF, 0b11]);
}

#[test]
fn a_loaded_lender_starts_after_the_reserved_numbers() {
    let mut lender = Lender::from_bytes(&[0b0001_0000], 8, 2).unwrap();
    let got: Vec<u32> = std::iter::from_fn(|| lender.take()).collect();
    check!("takes", got, vec![2, 3, 5, 6, 7]);
}

#[test]
fn the_last_free_number_is_found_wherever_it_is() {
    for hole in [2, 7, 8, 9, 15, 16, 40] {
        let mut lender = Lender::new(41, 2);
        while lender.take().is_some() {}
        lender.give_back(hole).unwrap();
        check!(format!("hole at {hole}"), (lender.take(), lender.take()), (Some(hole), None));
    }
}

#[test]
fn a_number_given_back_just_behind_is_found_after_a_full_round() {
    let mut lender = Lender::new(24, 0);
    for _ in 0..20 {
        lender.take();
    }
    lender.give_back(19).unwrap();
    let got: Vec<Option<u32>> = (0..6).map(|_| lender.take()).collect();
    check!("20 to 23, then 19", got, vec![Some(20), Some(21), Some(22), Some(23), Some(19), None]);
}

#[test]
fn a_million_numbers() {
    let mut lender = Lender::new(1_000_000, 17);
    let mut want = 17;
    while let Some(n) = lender.take() {
        assert_eq!(n, want);
        want += 1;
    }
    check!("all lent", (want, lender.free()), (1_000_000, 0));
    for n in (17..1_000_000).step_by(1000) {
        lender.give_back(n).unwrap();
    }
    check!("free", lender.free(), 1000);
    let again: Vec<u32> = std::iter::from_fn(|| lender.take()).collect();
    check!("each came back once, in order", again, (17..1_000_000).step_by(1000).collect::<Vec<u32>>());
}

#[test]
fn random_use_matches_a_set() {
    let mut rng = anneal_prelude::Rng::new(111_201);
    for round in 0..200 {
        let count = rng.below(50) as u32;
        let reserved = rng.below(count as usize + 2) as u32;
        let mut lender = Lender::new(count, reserved);
        let mut lent: BTreeSet<u32> = BTreeSet::new();
        let first = reserved.min(count);
        let mut next = first;
        for step in 0..120 {
            let what = format!("round {round} step {step} (count {count}, reserved {reserved})");
            if rng.below(5) < 3 {
                let free = |n: &u32| *n >= first && !lent.contains(n);
                let want = (next..count).chain(0..next.min(count)).find(free);
                check!(format!("{what}: take"), lender.take(), want);
                if let Some(n) = want {
                    lent.insert(n);
                    next = n + 1;
                }
            } else {
                let n = rng.below(count as usize + 3) as u32;
                let want = if n >= count {
                    Err(GiveBackError::OutOfRange)
                } else if n < first {
                    Err(GiveBackError::Reserved)
                } else if lent.remove(&n) {
                    Ok(())
                } else {
                    Err(GiveBackError::NotLent)
                };
                check!(format!("{what}: give back {n}"), lender.give_back(n), want);
            }
            check!(format!("{what}: free"), lender.free(), count - first - lent.len() as u32);
        }
        let mut map = vec![0u8; count.div_ceil(8) as usize];
        for n in (0..first).chain(lent.iter().copied()) {
            map[n as usize / 8] |= 1 << (n % 8);
        }
        check!(format!("round {round}: bytes"), lender.bytes().to_vec(), map);
    }
}
