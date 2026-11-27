use solution::*;

fn entry(id: u64, score: f64) -> Entry {
    Entry::new(id, score).expect("a score")
}

fn parts(list: Vec<Entry>) -> Vec<(u64, f64)> {
    list.iter().map(Entry::parts).collect()
}

fn run(k: usize, offers: &[(u64, f64)]) -> Vec<(u64, f64)> {
    let mut b = Best::new(k);
    for &(id, s) in offers {
        b.offer(entry(id, s));
    }
    parts(b.into_sorted())
}

#[test]
fn the_best_three() {
    check!(
        "k = 3",
        run(3, &[(1, 10.0), (2, 50.0), (3, 20.0), (4, 40.0), (5, 5.0)]),
        vec![(2, 50.0), (4, 40.0), (3, 20.0)]
    );
}

#[test]
fn fewer_offers_than_k() {
    check!("k = 10", run(10, &[(1, 1.0), (2, 2.0)]), vec![(2, 2.0), (1, 1.0)]);
}

#[test]
fn equal_scores_prefer_the_smaller_id() {
    check!("k = 2", run(2, &[(9, 1.0), (3, 1.0), (5, 1.0)]), vec![(3, 1.0), (5, 1.0)]);
}

#[test]
fn any_ordered_item() {
    let mut b = Best::new(2);
    for word in ["pear", "apple", "zebra", "mango"] {
        b.offer(word);
    }
    check!("the two greatest words", b.into_sorted(), vec!["zebra", "pear"]);
}

#[test]
fn offer_says_whether_it_stayed() {
    let mut b = Best::new(2);
    check!(
        "offers",
        [b.offer(5), b.offer(7), b.offer(6), b.offer(1), b.offer(6)],
        [true, true, true, false, false]
    );
}

#[test]
fn the_cutoff_appears_when_full() {
    let mut b = Best::new(2);
    check!("empty", b.cutoff(), None);
    b.offer(5);
    check!("one of two", b.cutoff(), None);
    b.offer(7);
    check!("full", b.cutoff(), Some(&5));
    b.offer(6);
    check!("after a better one", b.cutoff(), Some(&6));
}

#[test]
fn nan_is_not_a_score() {
    check!("NaN", Entry::new(1, f64::NAN).is_none(), true);
    check!("negative NaN", Entry::new(1, -f64::NAN).is_none(), true);
    check!("infinity", Entry::new(1, f64::INFINITY).map(|e| e.parts()), Some((1, f64::INFINITY)));
}
