use solution::*;

#[test]
fn the_raw_rolls_follow_the_recipe() {
    let mut dice = Dice::new(0);
    check!("first roll of seed 0", dice.roll(), 0xE220_A839_7B1D_CDAF_u64);
    check!("second roll of seed 0", dice.roll(), 0x6E78_9E6A_A1B9_65F4_u64);
}

#[test]
fn the_same_seed_replays_the_same_rolls() {
    let (mut a, mut b) = (Dice::new(77), Dice::new(77));
    for i in 0..100 {
        check!(format!("roll {i}"), a.between(3, 900), b.between(3, 900));
    }
}

#[test]
fn both_ends_of_a_range_come_out() {
    let mut dice = Dice::new(1);
    let mut seen = [0u32; 6];
    for _ in 0..600 {
        let v = dice.between(10, 15);
        ensure_in(v, 10, 15);
        seen[(v - 10) as usize] += 1;
    }
    check!("every value of 10..=15 seen", seen.iter().all(|&n| n > 50), true);
}

#[test]
fn a_range_of_one_value() {
    let mut dice = Dice::new(5);
    check!("between(4, 4)", dice.between(4, 4), 4);
    check!("the top value", dice.between(u64::MAX, u64::MAX), u64::MAX);
}

#[test]
fn zero_and_a_hundred_percent() {
    let mut dice = Dice::new(2);
    check!("percent(0)", (0..1000).any(|_| dice.percent(0)), false);
    check!("percent(100)", (0..1000).all(|_| dice.percent(100)), true);
    check!("percent(250)", (0..100).all(|_| dice.percent(250)), true);
}

#[test]
fn a_stream_replays_too() {
    let (mut a, mut b) = (Dice::stream(9, 4), Dice::stream(9, 4));
    check!("same pair", a.roll(), b.roll());
    check!("another id", Dice::stream(9, 4).roll() != Dice::stream(9, 5).roll(), true);
}

#[allow(dead_code)]
fn ensure_in(v: u64, lo: u64, hi: u64) {
    check!(format!("{v} lies in {lo}..={hi}"), (lo..=hi).contains(&v), true);
}
