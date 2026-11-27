use solution::*;
use std::collections::BTreeMap;

fn near(got: f64, want: f64) -> bool {
    if want < 1e-300 {
        (0.0..=1e-300).contains(&got)
    } else {
        got.is_finite() && (got - want).abs() <= want * 1e-5
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_billion_half_lives_later() {
    let mut c = Chart::new(1);
    c.report("a", 3, 999_999_990);
    c.report("b", 5, 999_999_990);
    c.report("c", 1, 999_999_994);
    check!("scores are finite", (near(c.score("a", 1_000_000_000), 3.0 / 1024.0), near(c.score("c", 1_000_000_000), 1.0 / 64.0)), (true, true));
    check!("order", c.top(9), names(&["c", "b", "a"]));
}

#[test]
fn long_forgotten_songs_keep_their_order() {
    let mut c = Chart::new(1);
    c.report("a", 1, 10);
    c.report("b", 2, 10);
    c.report("c", 1, 5_000);
    c.report("d", 1, 900_000_000);
    check!("order", c.top(9), names(&["d", "c", "b", "a"]));
    check!("too small to show", near(c.score("a", 900_000_000), 0.0), true);
}

#[test]
fn nothing_overflows_after_years_of_reports() {
    let mut c = Chart::new(3600);
    for day in 0..2000u64 {
        c.report("daily", 1000, day * 86_400);
    }
    let want = 1000.0 / (1.0 - 0.5f64.powi(24));
    check!("steady score", near(c.score("daily", 1999 * 86_400), want), true);
}

#[test]
fn same_reports_tie_by_name() {
    let mut c = Chart::new(7);
    for name in ["pear", "apple", "mango", "fig", "kiwi", "date", "lime", "plum", "sloe", "yuzu"] {
        c.report(name, 3, 100);
        c.report(name, 9, 40);
    }
    check!("by name", c.top(10), names(&["apple", "date", "fig", "kiwi", "lime", "mango", "pear", "plum", "sloe", "yuzu"]));
    check!("only three", c.top(3), names(&["apple", "date", "fig"]));
    check!("none", c.top(0), Vec::<String>::new());
    check!("huge n", c.top(usize::MAX).len(), 10);
}

#[test]
fn the_longest_half_life() {
    let mut c = Chart::new(1_000_000_000);
    c.report("a", u32::MAX, 0);
    c.report("a", u32::MAX, 0);
    c.report("b", 1, 1_000_000_000);
    check!("a", near(c.score("a", 1_000_000_000), f64::from(u32::MAX)), true);
    check!("order", c.top(2), names(&["a", "b"]));
}

#[test]
fn a_hundredth_of_a_percent_apart() {
    let mut c = Chart::new(3600);
    c.report("lower", 100_000, 777_777);
    c.report("upper", 100_010, 777_777);
    check!("order", c.top(2), names(&["upper", "lower"]));
}

/// The sum from the statement, over the kept reports.
fn exact(reports: &[(u32, u64)], half_life: u64, now: u64) -> f64 {
    reports.iter().map(|&(p, at)| f64::from(p) * (-((now - at) as f64) / half_life as f64).exp2()).sum()
}

#[test]
fn agrees_with_adding_up_every_report() {
    let mut rng = anneal_prelude::Rng::new(15106);
    let songs = ["a", "b", "c", "d", "e"];
    for round in 0..400 {
        let half_life = 1 + rng.below(50) as u64;
        // early, middle and very late in the life of the station
        let base = [0u64, 5_000_000, 999_000_000][round % 3];
        let mut c = Chart::new(half_life);
        let mut kept: BTreeMap<&str, Vec<(u32, u64)>> = BTreeMap::new();
        let mut latest = base;
        for _ in 0..rng.below(30) {
            let song = *rng.pick(&songs);
            let plays = rng.below(6) as u32;
            let at = base + rng.below(400) as u64;
            latest = latest.max(at);
            c.report(song, plays, at);
            if plays > 0 {
                kept.entry(song).or_default().push((plays, at));
            }
        }
        let now = latest + rng.below(100) as u64;
        for song in songs {
            let want = exact(kept.get(song).map_or(&[][..], |v| v), half_life, now);
            let got = c.score(song, now);
            check!(format!("half_life {half_life}, score of {song} at {now}: got {got}, exact {want}"), near(got, want), true);
        }
        let top = c.top(10);
        let mut sorted = top.clone();
        sorted.sort();
        check!("the songs listed", sorted, kept.keys().map(|s| s.to_string()).collect::<Vec<_>>());
        // compared at the latest report, where the larger scores are far from underflow
        for pair in top.windows(2) {
            let (first, second) = (&kept[pair[0].as_str()], &kept[pair[1].as_str()]);
            let (a, b) = (exact(first, half_life, latest), exact(second, half_life, latest));
            check!(format!("{pair:?} score {a} and {b}"), b <= a * (1.0 + 1e-5), true);
            if first == second {
                check!(format!("tie {pair:?}"), pair[0] < pair[1], true);
            }
        }
    }
}

#[test]
fn many_small_reports_equal_one_large_one() {
    let mut c = Chart::new(1000);
    for _ in 0..2000 {
        c.report("small", 1, 500_000_000);
    }
    c.report("large", 2000, 500_000_000);
    check!("same score", (near(c.score("small", 500_000_000), 2000.0), near(c.score("large", 500_000_000), 2000.0)), (true, true));
    c.report("small", 1, 500_000_000);
    check!("one more play wins", c.top(2), vec!["small".to_string(), "large".to_string()]);
}
