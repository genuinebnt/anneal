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
fn an_error_stops_the_search() {
    let mut feed = Written::default();
    feed.days.insert(2, Err(FeedError { day: 2 }));
    feed.days.insert(3, Ok(vec![r(1, 1, 5), r(1, 2, 4)]));
    check!("the error", first_bad_day(&mut feed, &PLAIN, 0..9), Err(FeedError { day: 2 }));
    check!("nothing after it", feed.asked, vec![0, 1, 2]);
}

#[test]
fn a_bad_day_before_the_error_wins() {
    let mut feed = Written::default();
    feed.days.insert(0, Ok(vec![r(1, 1, 5), r(1, 2, 4)]));
    feed.days.insert(1, Err(FeedError { day: 1 }));
    check!("day 0", first_bad_day(&mut feed, &PLAIN, 0..9).map(|b| b.map(|b| b.day)), Ok(Some(0)));
}

#[test]
fn an_error_is_not_remembered() {
    let mut feed = Written::default();
    feed.days.insert(4, Err(FeedError { day: 4 }));
    {
        let mut remembering = Remembering::new(&mut feed);
        check!("fails", remembering.day(4), Err(FeedError { day: 4 }));
        check!("fails again", remembering.day(4), Err(FeedError { day: 4 }));
    }
    check!("asked both times", feed.asked, vec![4, 4]);
}

#[test]
fn a_remembering_feed_works_in_the_search() {
    let mut feed = Written::default();
    {
        let mut remembering = Remembering::new(&mut feed);
        check!("clean", first_bad_day(&mut remembering, &PLAIN, 0..3), Ok(None));
        check!("clean again", first_bad_day(&mut remembering, &PLAIN, 0..4), Ok(None));
    }
    check!("each day once", feed.asked, vec![0, 1, 2, 3]);
}

#[test]
fn the_first_in_list_order_not_the_lowest_meter() {
    let mut feed = Written::default();
    let day = vec![r(1, 1, 9), r(2, 1, 9), r(2, 2, 1), r(1, 2, 1)];
    feed.days.insert(0, Ok(day.clone()));
    check!("meter 2 came first", first_bad_day(&mut feed, &PLAIN, 0..1), Ok(Some(BadDay { day: 0, before: day[1], after: day[2] })));
    check!("trusted", trusted_start(&day, &PLAIN).len(), 2);
}

#[test]
fn a_day_does_not_remember_the_day_before() {
    let mut feed = Written::default();
    feed.days.insert(0, Ok(vec![r(1, 1, 50)]));
    feed.days.insert(1, Ok(vec![r(1, 2, 3)]));
    check!("separate days", first_bad_day(&mut feed, &PLAIN, 0..2), Ok(None));
}

#[test]
fn equal_totals_and_huge_tolerances() {
    let list = [r(1, 1, 5), r(1, 2, 5)];
    check!("allowed", first_break(&list, &PLAIN), None);
    check!("strict", first_break(&list, &PLAIN.strict()), Some((list[0], list[1])));
    let fall = [r(1, 1, u64::MAX), r(1, 2, 0)];
    check!("the largest fall is within the largest tolerance", first_break(&fall, &Rules { tolerance: u64::MAX, ..PLAIN }), None);
    check!("one less is not", first_break(&fall, &Rules { tolerance: u64::MAX - 1, ..PLAIN }).is_some(), true);
    let small = [r(1, 1, 10), r(1, 2, 8)];
    check!("a small fall with the largest tolerance", first_break(&small, &Rules { tolerance: u64::MAX, ..PLAIN }), None);
    let high = [r(1, 1, 3), r(1, 2, 5)];
    check!("a rise with a huge tolerance", first_break(&high, &Rules { tolerance: u64::MAX, ..PLAIN }), None);
}

#[test]
fn the_newest_by_time_then_total() {
    let meters = by_meter(&[r(1, 8, 2), r(1, 8, 6), r(1, 8, 4), r(1, 2, 100)]);
    check!("largest total at the latest time", current_total(&meters, 1, 8, &PLAIN), Some(6));
    let aged = by_meter(&[r(1, 100, 1), r(1, 0, 2)]);
    check!("an older one does not stand in", current_total(&aged, 1, 5, &PLAIN), None);
}

#[test]
fn nothing_to_check() {
    check!("no readings", by_meter(&[]).len(), 0);
    check!("no pair", first_break(&[r(1, 1, 1)], &PLAIN), None);
    check!("whole empty day", trusted_start(&[], &PLAIN).len(), 0);
    check!("empty range", first_bad_day(&mut Written::default(), &PLAIN, 5..5), Ok(None));
}

#[test]
fn matches_comparing_by_hand() {
    let mut rng = anneal_prelude::Rng::new(41_401);
    let mut found = 0;
    for round in 0..600 {
        let rules = Rules { allow_equal: rng.bool(), tolerance: rng.int(0, 3) as u64, max_age: rng.int(0, 6) as u64 };
        let n = rng.below(10);
        let day: Vec<Reading> = (0..n).map(|_| r(rng.below(3) as u32, rng.int(0, 9) as u64, rng.int(0, 9) as u64)).collect();
        let label = format!("round {round}: {rules:?} {day:?}");
        let bad = |a: Reading, b: Reading| {
            b.total as i64 + (rules.tolerance as i64) < a.total as i64 || (!rules.allow_equal && a.total == b.total)
        };
        // for each reading, search backwards for the one before it
        let mut want = None;
        for j in 0..day.len() {
            if let Some(i) = (0..j).rev().find(|&i| day[i].meter == day[j].meter) {
                if bad(day[i], day[j]) {
                    want = Some((i, j));
                    break;
                }
            }
        }
        found += usize::from(want.is_some());
        let keep = want.map_or(day.len(), |w| w.1);
        check!(format!("{label}: trusted start"), trusted_start(&day, &rules).to_vec(), day[..keep].to_vec());
        let mut feed = Written::default();
        feed.days.insert(0, Ok(day.clone()));
        let expected = want.map(|(i, j)| BadDay { day: 0, before: day[i], after: day[j] });
        check!(format!("{label}: first bad day"), first_bad_day(&mut feed, &rules, 0..2), Ok(expected));

        let meters = by_meter(&day);
        for meter in 0..3 {
            let own: Vec<Reading> = day.iter().copied().filter(|x| x.meter == meter).collect();
            check!(format!("{label}: meter {meter}"), meters.get(&meter).cloned().unwrap_or_default(), own.clone());
            check!(format!("{label}: entry only if it has readings"), meters.contains_key(&meter), !own.is_empty());
            let pair = (1..own.len()).find(|&j| bad(own[j - 1], own[j])).map(|j| (own[j - 1], own[j]));
            check!(format!("{label}: first break of {meter}"), first_break(&own, &rules), pair);
            let now = rng.int(0, 12) as u64;
            let mut sorted = own.clone();
            sorted.sort_by_key(|x| (x.at, x.total));
            let total = sorted.last().filter(|x| x.at <= now && now - x.at <= rules.max_age).map(|x| x.total);
            check!(format!("{label}: total of {meter} at {now}"), current_total(&meters, meter, now, &rules), total);
        }
    }
    check!("many days had a break", found > 100, true);
}

#[test]
fn a_big_day_is_checked_quickly() {
    let mut day: Vec<Reading> = (0..200_000u64).map(|i| r((i % 50_000) as u32, i, i)).collect();
    day.push(r(49_999, 200_000, 0));
    check!("only the last breaks", trusted_start(&day, &PLAIN).len(), 200_000);
    check!("meters", by_meter(&day).len(), 50_000);
}
