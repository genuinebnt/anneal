use solution::*;

/// Sends `rounds` batches to a service that accepts a batch when it is no larger than `capacity`.
/// Returns (refused rounds, units accepted, largest accepted batch).
fn run(b: &mut Batcher, max: u32, capacity: u32, rounds: u32) -> (u32, u64, u32) {
    let (mut refused, mut units, mut largest) = (0, 0u64, 0);
    for round in 0..rounds {
        let size = b.next_size();
        assert!(size >= 1 && size <= max, "round {round}: size {size} is outside 1..={max} (capacity {capacity})");
        let ok = size <= capacity;
        b.report(ok);
        if ok {
            units += u64::from(size);
            largest = largest.max(size);
        } else {
            refused += 1;
        }
    }
    (refused, units, largest)
}

/// Over the next 2000 rounds: at most 15 in 100 refused, and at least 60% of what a sender who knew `capacity` would get through.
fn check_steady(b: &mut Batcher, max: u32, capacity: u32, story: &str) {
    let (refused, units, _) = run(b, max, capacity, 2000);
    let best = 2000 * u64::from(capacity.min(max));
    assert!(refused <= 300, "{story}, capacity {capacity}: {refused} of 2000 batches refused (at most 300)");
    assert!(units * 100 >= best * 60, "{story}, capacity {capacity}: {units} units accepted, under 60% of {best}");
}

#[test]
fn asking_twice_gives_the_same_size() {
    let mut b = Batcher::new(50);
    for _ in 0..30 {
        let first = b.next_size();
        check!("asked again", b.next_size(), first);
        b.report(first <= 9);
    }
}

#[test]
fn a_steady_service() {
    for capacity in [20, 100, 500] {
        let mut b = Batcher::new(1000);
        run(&mut b, 1000, capacity, 1500);
        check_steady(&mut b, 1000, capacity, "steady");
    }
}

#[test]
fn a_sudden_fall() {
    let mut b = Batcher::new(4000);
    run(&mut b, 4000, 1000, 5000);
    let (refused, _, _) = run(&mut b, 4000, 10, 16);
    check!("something got through in 16 rounds", refused < 16, true);
}

#[test]
fn a_rise() {
    let mut b = Batcher::new(1000);
    run(&mut b, 1000, 20, 1500);
    let (_, _, largest) = run(&mut b, 1000, 600, 700);
    check!(format!("largest accepted in 700 rounds was {largest}"), largest >= 400, true);
}

#[test]
fn room_to_spare() {
    let mut b = Batcher::new(100);
    run(&mut b, 100, 1_000_000, 200);
    let (_, units, _) = run(&mut b, 100, 1_000_000, 2000);
    check!(format!("{units} units of 200000"), units >= 190_000, true);
}
