use solution::*;


#[allow(dead_code)]
fn pairs(items: &[(&str, i64)]) -> Vec<(String, i64)> {
    items.iter().map(|(n, s)| (n.to_string(), *s)).collect()
}
#[allow(dead_code)]
fn board(items: &[(&str, i64)]) -> Board {
    let mut b = Board::new();
    for (name, score) in items {
        b.set(name, *score);
    }
    b
}

#[test]
fn ordered_by_score_then_name() {
    let b = board(&[("cat", 5), ("bob", 2), ("ann", 5), ("dan", -1)]);
    check!("the whole board", b.range(0, 10), pairs(&[("dan", -1), ("bob", 2), ("ann", 5), ("cat", 5)]));
    check!("ann's place", b.rank("ann"), Some(2));
    check!("two from place 1", b.range(1, 2), pairs(&[("bob", 2), ("ann", 5)]));
}

#[test]
fn a_new_score_moves_the_player() {
    let mut b = board(&[("ann", 1), ("bob", 2)]);
    check!("the old score", b.set("ann", 3), Some(1));
    check!("ann is last now", b.rank("ann"), Some(1));
    check!("still two players", b.len(), 2);
    check!("ann's score", b.score("ann"), Some(3));
}

#[test]
fn removing() {
    let mut b = board(&[("ann", 1), ("bob", 2), ("cat", 3)]);
    check!("remove bob", b.remove("bob"), Some(2));
    check!("again", b.remove("bob"), None);
    check!("cat moves up", b.rank("cat"), Some(1));
    check!("bob has no place", b.rank("bob"), None);
}

#[test]
fn counting_below_a_score() {
    let b = board(&[("a", 10), ("b", 20), ("c", 20), ("d", 30)]);
    check!("below 20", b.count_below(20), 1);
    check!("below 21", b.count_below(21), 3);
    check!("below the lowest", b.count_below(i64::MIN), 0);
}

#[test]
fn empty_and_past_the_end() {
    check!("an empty board", Board::new().range(0, 5), vec![]);
    let b = board(&[("a", 1)]);
    check!("past the end", b.range(1, 5), vec![]);
    check!("none asked for", b.range(0, 0), vec![]);
    check!("far past the end", b.range(usize::MAX, usize::MAX), vec![]);
}
