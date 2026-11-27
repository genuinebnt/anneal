use solution::*;
use std::collections::HashMap;

#[allow(dead_code)]
fn r(meter: u32, at: u64, total: u64) -> Reading {
    Reading { meter, at, total }
}

#[allow(dead_code)]
const PLAIN: Rules = Rules {
    allow_equal: true,
    tolerance: 0,
    max_age: 10,
};

/// Days written down in advance. It remembers which days were asked for.
#[derive(Default)]
struct Written {
    days: HashMap<u64, Result<Vec<Reading>, FeedError>>,
    asked: Vec<u64>,
}

impl Feed for Written {
    fn day(&mut self, day: u64) -> Result<Vec<Reading>, FeedError> {
        self.asked.push(day);
        self.days.get(&day).cloned().unwrap_or(Ok(Vec::new()))
    }
}

#[test]
fn readings_go_to_their_meters() {
    let meters = by_meter(&[r(2, 1, 5), r(7, 1, 9), r(2, 2, 6)]);
    check!("two meters", meters.len(), 2);
    check!("meter 2 in list order", meters.get(&2), Some(&vec![r(2, 1, 5), r(2, 2, 6)]));
}

#[test]
fn only_a_fresh_newest_reading_counts() {
    let meters = by_meter(&[r(1, 20, 7), r(1, 5, 3)]);
    check!("fresh at 30", current_total(&meters, 1, 30, &PLAIN), Some(7));
    check!("out of date at 31", current_total(&meters, 1, 31, &PLAIN), None);
    check!("from the future", current_total(&meters, 1, 19, &PLAIN), None);
    check!("unknown meter", current_total(&meters, 9, 30, &PLAIN), None);
}

#[test]
fn the_first_two_neighbours_that_break() {
    let list = [r(1, 1, 5), r(1, 2, 8), r(1, 3, 6), r(1, 4, 2)];
    check!("8 then 6", first_break(&list, &PLAIN), Some((r(1, 2, 8), r(1, 3, 6))));
    check!("a fall of 2 is allowed", first_break(&list, &Rules { tolerance: 2, ..PLAIN }), Some((r(1, 3, 6), r(1, 4, 2))));
}

#[test]
fn strict_changes_two_settings() {
    let loose = Rules { allow_equal: true, tolerance: 4, max_age: 99 };
    check!("strict", loose.strict(), Rules { allow_equal: false, tolerance: 0, max_age: 99 });
}

#[test]
fn meters_are_judged_apart() {
    let day = [r(1, 1, 100), r(2, 1, 5), r(1, 2, 101), r(2, 2, 4), r(1, 3, 102)];
    check!("up to meter 2's fall", trusted_start(&day, &PLAIN).to_vec(), day[..3].to_vec());
}

#[test]
fn the_first_bad_day_ends_the_search() {
    let mut feed = Written::default();
    feed.days.insert(3, Ok(vec![r(1, 1, 5), r(1, 2, 4)]));
    let found = first_bad_day(&mut feed, &PLAIN, 0..u64::MAX);
    check!("day 3", found, Ok(Some(BadDay { day: 3, before: r(1, 1, 5), after: r(1, 2, 4) })));
    check!("no later day was asked for", feed.asked, vec![0, 1, 2, 3]);
}

#[test]
fn an_answer_is_asked_for_once() {
    let mut feed = Written::default();
    feed.days.insert(1, Ok(vec![r(1, 1, 5)]));
    let mut remembering = Remembering::new(&mut feed);
    check!("first", remembering.day(1), Ok(vec![r(1, 1, 5)]));
    check!("again", remembering.day(1), Ok(vec![r(1, 1, 5)]));
    check!("asked once", feed.asked, vec![1]);
}
