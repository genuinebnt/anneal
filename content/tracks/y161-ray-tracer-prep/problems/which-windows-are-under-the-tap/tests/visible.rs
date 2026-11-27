#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    Rect { x0, y0, x1, y1 }
}

#[allow(dead_code)]
fn has_point(r: &Rect, x: f64, y: f64) -> bool {
    r.x0 <= x && x <= r.x1 && r.y0 <= y && y <= r.y1
}

#[allow(dead_code)]
fn overlaps(r: &Rect, q: &Rect) -> bool {
    r.x0 <= q.x1 && q.x0 <= r.x1 && r.y0 <= q.y1 && q.y0 <= r.y1
}

/// Runs a search and counts the questions.
#[allow(dead_code)]
fn ask(index: &Index, mut question: impl FnMut(&Rect) -> bool) -> (Vec<usize>, usize) {
    let mut calls = 0;
    let found = index.search(&mut |r| {
        calls += 1;
        question(r)
    });
    (found, calls)
}

/// `side * side` small squares, 2 apart, listed in a scrambled order.
#[allow(dead_code)]
fn tiles(side: usize) -> Vec<Rect> {
    let mut all: Vec<Rect> = (0..side * side)
        .map(|i| {
            let (x, y) = ((i % side) as f64 * 2.0, (i / side) as f64 * 2.0);
            rect(x, y, x + 1.0, y + 1.0)
        })
        .collect();
    Rng::new(7).shuffle(&mut all);
    all
}

#[test]
fn a_tap_on_a_few_windows() {
    let index = Index::new(&[
        rect(0.0, 0.0, 4.0, 4.0),
        rect(2.0, 2.0, 6.0, 6.0),
        rect(5.0, 0.0, 7.0, 1.0),
    ]);
    check!(
        "in two",
        ask(&index, |r| has_point(r, 3.0, 3.0)).0,
        vec![0, 1]
    );
    check!("in one", ask(&index, |r| has_point(r, 6.0, 0.5)).0, vec![2]);
    check!(
        "in none",
        ask(&index, |r| has_point(r, 9.0, 9.0)).0,
        Vec::<usize>::new()
    );
}

#[test]
fn no_windows() {
    check!(
        "nothing",
        ask(&Index::new(&[]), |_| true).0,
        Vec::<usize>::new()
    );
}

#[test]
fn another_kind_of_question() {
    let index = Index::new(&[
        rect(0.0, 0.0, 1.0, 1.0),
        rect(3.0, 3.0, 4.0, 4.0),
        rect(8.0, 8.0, 9.0, 9.0),
    ]);
    // Does the rectangle reach the region below the line x + y = 6.5?
    check!(
        "below the line",
        ask(&index, |r| r.x0 + r.y0 <= 6.5).0,
        vec![0, 1]
    );
}

#[test]
fn every_window() {
    let index = Index::new(&tiles(10));
    check!(
        "all, in rising order",
        ask(&index, |_| true).0,
        (0..100).collect::<Vec<_>>()
    );
}

#[test]
fn one_tap_among_thousands_asks_little() {
    let all = tiles(64);
    let index = Index::new(&all);
    let (found, calls) = ask(&index, |r| has_point(r, 40.5, 86.5));
    check!("one tile", found.len(), 1);
    check!("the right one", all[found[0]], rect(40.0, 86.0, 41.0, 87.0));
    check!(
        format!("at most 100 questions for 4096 tiles, asked {calls}"),
        calls <= 100,
        true
    );
}
