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
fn the_first_batch_is_valid() {
    for max in [1, 2, 7, 1000, u32::MAX] {
        let size = Batcher::new(max).next_size();
        check!(format!("max {max}: first size {size}"), size >= 1 && size <= max, true);
    }
}

#[test]
fn a_small_max_under_a_large_capacity() {
    for max in [1, 2, 3, 10, 64] {
        let mut b = Batcher::new(max);
        run(&mut b, max, max, max + 100);
        let (_, units, _) = run(&mut b, max, max, 2000);
        check!(format!("max {max}: {units} units"), units * 100 >= 2000 * u64::from(max) * 95, true);
    }
}

#[test]
fn the_service_recovers_twice() {
    let mut b = Batcher::new(1000);
    for _ in 0..2 {
        run(&mut b, 1000, 20, 1500);
        let (_, _, largest) = run(&mut b, 1000, 600, 700);
        check!(format!("largest accepted was {largest}"), largest >= 400, true);
        let (refused, _, _) = run(&mut b, 1000, 10, 16);
        check!("back down within 16 rounds", refused < 16, true);
    }
}

#[test]
fn every_steady_capacity() {
    for capacity in 20..=500 {
        let mut b = Batcher::new(1000);
        run(&mut b, 1000, capacity, 1500);
        check_steady(&mut b, 1000, capacity, "steady");
    }
}

#[test]
fn falls_from_every_height() {
    for high in [200, 640, 999, 1000, 1024, 2047] {
        let mut b = Batcher::new(4000);
        run(&mut b, 4000, high, 5000);
        // at several points of whatever pattern the rule settles into
        for extra in 0..40 {
            run(&mut b, 4000, high, extra);
            let (refused, _, _) = run(&mut b, 4000, 10, 16);
            check!(format!("fall from {high} after {extra} more rounds"), refused < 16, true);
            run(&mut b, 4000, high, 3000);
        }
    }
}

#[test]
fn an_outage_and_the_way_back() {
    let mut b = Batcher::new(1000);
    run(&mut b, 1000, 300, 2000);
    let (refused, _, _) = run(&mut b, 1000, 0, 5000);
    check!("all refused", refused, 5000);
    let (_, _, largest) = run(&mut b, 1000, 600, 700);
    check!(format!("largest accepted after the outage was {largest}"), largest >= 400, true);
    run(&mut b, 1000, 90, 1500);
    check_steady(&mut b, 1000, 90, "after an outage");
}

#[test]
fn a_service_that_keeps_changing() {
    let mut rng = anneal_prelude::Rng::new(9104);
    for story in 0..30 {
        let mut b = Batcher::new(1000);
        let mut history = format!("story {story}: capacities");
        for _ in 0..8 {
            let capacity = 20 + rng.below(481) as u32;
            history += &format!(" {capacity}");
            run(&mut b, 1000, capacity, 1500);
            check_steady(&mut b, 1000, capacity, &history);
        }
    }
}

#[test]
fn the_smallest_and_largest_max() {
    let mut one = Batcher::new(1);
    run(&mut one, 1, 0, 100);
    let (refused, _, _) = run(&mut one, 1, 5, 100);
    check!("nothing refused once capacity allows 1", refused <= 1, true);
    let mut huge = Batcher::new(u32::MAX);
    run(&mut huge, u32::MAX, u32::MAX, 100);
    run(&mut huge, u32::MAX, 0, 100);
    run(&mut huge, u32::MAX, u32::MAX, 100);
    run(&mut huge, u32::MAX, 300, 2000);
    check_steady(&mut huge, u32::MAX, 300, "max = u32::MAX");
}
