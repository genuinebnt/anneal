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
fn taps_all_over_the_tiles() {
    let all = tiles(64);
    let index = Index::new(&all);
    let mut rng = Rng::new(1633);
    for _ in 0..500 {
        let (x, y) = (
            rng.int(-10, 1380) as f64 / 10.0,
            rng.int(-10, 1380) as f64 / 10.0,
        );
        let (found, calls) = ask(&index, |r| has_point(r, x, y));
        let want: Vec<usize> = (0..all.len())
            .filter(|&i| has_point(&all[i], x, y))
            .collect();
        check!(format!("({x}, {y})"), found, want);
        check!(
            format!("({x}, {y}): asked {calls}, at most 100"),
            calls <= 100,
            true
        );
    }
}

#[test]
fn a_stripe_in_each_direction() {
    let index = Index::new(&tiles(64));
    for at in [0.5, 30.5, 64.5, 126.5] {
        let (found, calls) = ask(&index, |r| r.x0 <= at && at <= r.x1);
        check!(format!("upright stripe at {at}"), found.len(), 64);
        check!(
            format!("upright stripe at {at}: asked {calls}, at most 700"),
            calls <= 700,
            true
        );
        let (found, calls) = ask(&index, |r| r.y0 <= at && at <= r.y1);
        check!(format!("flat stripe at {at}"), found.len(), 64);
        check!(
            format!("flat stripe at {at}: asked {calls}, at most 700"),
            calls <= 700,
            true
        );
    }
}

#[test]
fn a_question_nothing_can_pass() {
    let index = Index::new(&tiles(64));
    let (found, calls) = ask(&index, |r| has_point(r, -50.0, -50.0));
    check!("nothing", found.len(), 0);
    check!(
        format!("far from everything: asked {calls}, at most 4"),
        calls <= 4,
        true
    );
}

#[test]
fn a_question_everything_passes() {
    let index = Index::new(&tiles(64));
    let (found, calls) = ask(&index, |_| true);
    check!("all", found.len(), 4096);
    check!(
        format!("asked {calls}, at most 3 per tile"),
        calls <= 3 * 4096,
        true
    );
}

#[test]
fn only_its_answer_counts() {
    // The question is stricter than any geometry: only rectangles of one exact width pass at the end,
    // while groups pass if they are at least that wide.
    let mut all = tiles(8);
    all[5] = rect(100.0, 100.0, 103.0, 101.0);
    let index = Index::new(&all);
    check!(
        "the wide one",
        ask(&index, |r| r.x1 - r.x0 >= 3.0 && r.x1 >= 103.0).0,
        vec![5]
    );
    check!("always no", ask(&index, |_| false).0, Vec::<usize>::new());
}

#[test]
fn tall_thin_and_short_wide() {
    for tall in [true, false] {
        let all: Vec<Rect> = (0..4096)
            .map(|i| {
                let at = 2.0 * i as f64;
                if tall {
                    rect(0.0, at, 1000.0, at + 1.0)
                } else {
                    rect(at, 0.0, at + 1.0, 1000.0)
                }
            })
            .collect();
        let index = Index::new(&all);
        let (found, calls) = ask(&index, |r| {
            if tall {
                has_point(r, 500.0, 4000.5)
            } else {
                has_point(r, 4000.5, 500.0)
            }
        });
        check!("one bar", found, vec![2000]);
        check!(
            format!("tall {tall}: asked {calls}, at most 100"),
            calls <= 100,
            true
        );
    }
}

#[test]
fn all_in_one_place() {
    let all = vec![rect(1.0, 1.0, 2.0, 2.0); 30_000];
    let index = Index::new(&all);
    check!(
        "every copy",
        ask(&index, |r| has_point(r, 1.5, 1.5)).0 == (0..30_000).collect::<Vec<_>>(),
        true
    );
    check!(
        "or none",
        ask(&index, |r| has_point(r, 5.0, 5.0)).0.len(),
        0
    );
}

#[test]
fn broken_numbers_do_not_panic_or_hide_others() {
    let nan = f64::NAN;
    let mut all = tiles(8);
    for i in [0, 9, 10, 33, 63] {
        all[i] = rect(nan, nan, nan, nan);
    }
    let index = Index::new(&all);
    for (i, r) in all.iter().enumerate() {
        if r.x0.is_nan() {
            continue;
        }
        let found = ask(&index, |q| has_point(q, r.x0 + 0.5, r.y0 + 0.5)).0;
        check!(format!("tile {i}"), found, vec![i]);
    }
}

#[test]
fn random_windows() {
    let mut rng = Rng::new(1634);
    for case in 0..200 {
        let n = *rng.pick(&[1usize, 2, 4, 5, 9, 30, 100, 300]);
        let make = |rng: &mut Rng, big: i64| {
            let (x, y) = (rng.int(0, 100) as f64, rng.int(0, 100) as f64);
            rect(x, y, x + rng.int(0, big) as f64, y + rng.int(0, big) as f64)
        };
        let all: Vec<Rect> = (0..n).map(|_| make(&mut rng, 25)).collect();
        let index = Index::new(&all);
        for _ in 0..30 {
            let window = make(&mut rng, 20);
            let want: Vec<usize> = (0..n).filter(|&i| overlaps(&all[i], &window)).collect();
            check!(
                format!("case {case}: {n} windows against {window:?}"),
                ask(&index, |r| overlaps(r, &window)).0,
                want
            );
        }
    }
}
