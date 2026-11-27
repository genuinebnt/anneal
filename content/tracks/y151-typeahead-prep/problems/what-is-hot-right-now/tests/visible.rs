use solution::*;

fn near(got: f64, want: f64) -> bool {
    (got - want).abs() <= want.abs() * 1e-5
}

#[test]
fn a_play_halves_every_half_life() {
    let mut c = Chart::new(60);
    c.report("a", 8, 0);
    check!("at once", near(c.score("a", 0), 8.0), true);
    check!("one half-life", near(c.score("a", 60), 4.0), true);
    check!("three", near(c.score("a", 180), 1.0), true);
}

#[test]
fn part_of_a_half_life_counts() {
    let mut c = Chart::new(100);
    c.report("a", 10, 0);
    check!("half a half-life", near(c.score("a", 50), 10.0 * 0.5f64.sqrt()), true);
}

#[test]
fn new_plays_overtake_old_ones() {
    let mut c = Chart::new(10);
    c.report("old", 100, 0);
    c.report("new", 30, 20);
    check!("old is 25 at 20, new is 30", c.top(2), vec!["new".to_string(), "old".to_string()]);
}

#[test]
fn a_late_report_is_still_old() {
    let mut c = Chart::new(10);
    c.report("a", 1, 100);
    c.report("a", 4, 80);
    check!("1 + 4/4", near(c.score("a", 100), 2.0), true);
}

#[test]
fn unknown_and_unplayed_songs() {
    let mut c = Chart::new(10);
    c.report("silent", 0, 5);
    check!("unknown", c.score("nobody", 9), 0.0);
    check!("not listed", c.top(5), Vec::<String>::new());
}
