use solution::*;

#[allow(dead_code)]
fn rec(pos: u64, day: u32, shop: &str, kind: Kind, cents: u64) -> Record {
    Record { pos, day, shop: shop.to_string(), kind, cents }
}

#[allow(dead_code)]
fn small_log() -> Vec<Record> {
    vec![
        rec(0, 1, "north", Kind::Sale, 500),
        rec(1, 1, "south", Kind::Sale, 300),
        rec(2, 2, "north", Kind::Refund, 500),
        rec(3, 2, "south", Kind::Sale, 50),
    ]
}

#[test]
fn following_the_feed() {
    let log = small_log();
    let mut b = Board::new();
    check!("first two", b.apply(&log[..2]), Ok(2));
    check!("the rest", b.apply(&log[2..]), Ok(2));
    check!("north", b.net("north"), 0);
    check!("south", b.net("south"), 350);
    check!("next", b.next(), 4);
}

#[test]
fn a_refund_counts_on_its_own_day() {
    let b = Board::rebuild(&small_log());
    check!("day 1", b.day(1), (800, 0));
    check!("day 2", b.day(2), (50, 500));
    check!("no such day", b.day(9), (0, 0));
}

#[test]
fn a_batch_sent_again_counts_once() {
    let log = small_log();
    let mut b = Board::new();
    b.apply(&log[..3]).unwrap();
    check!("overlap", b.apply(&log[1..]), Ok(1));
    check!("all old", b.apply(&log[..2]), Ok(0));
    check!("next stays", b.next(), 4);
    check!("south", b.net("south"), 350);
}

#[test]
fn a_hole_is_refused() {
    let log = small_log();
    let mut b = Board::new();
    check!("starts at 2", b.apply(&log[2..]), Err(Gap { expected: 0, got: 2 }));
    check!("nothing counted", b, Board::new());
}

#[test]
fn the_best_shops() {
    let mut log = small_log();
    log.push(rec(4, 2, "east", Kind::Sale, 350));
    let b = Board::rebuild(&log);
    check!("top 2, a tie by name", b.top(2), vec![("east".to_string(), 350), ("south".to_string(), 350)]);
    check!("all, zero included", b.top(10).len(), 3);
    check!("none", b.top(0), vec![]);
}

#[test]
fn the_board_as_it_was() {
    let log = small_log();
    check!("before position 2", Board::as_of(&log, 2).net("north"), 500);
    check!("before 0", Board::as_of(&log, 0), Board::new());
    check!("past the end", Board::as_of(&log, u64::MAX), Board::rebuild(&log));
}
