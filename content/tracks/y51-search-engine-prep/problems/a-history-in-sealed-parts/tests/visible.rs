use solution::*;

#[test]
fn an_empty_history() {
    let h = History::new();
    check!("len, sizes", (h.len(), h.part_sizes()), (0, vec![]));
    check!("message 0, mean", (h.message(0), h.mean_length()), (None, None));
    check!("by_author", h.by_author(1), Vec::<usize>::new());
}

#[test]
fn one_message_at_a_time() {
    let mut h = History::new();
    let mut seen = Vec::new();
    for _ in 0..5 {
        h.append(&[(1, "hi")]);
        seen.push(h.part_sizes());
    }
    check!(
        "sizes after each of five appends",
        seen,
        vec![vec![1], vec![2], vec![2, 1], vec![4], vec![4, 1]]
    );
}

#[test]
fn numbers_run_across_batches() {
    let mut h = History::new();
    h.append(&[(7, "a"), (8, "bb"), (7, "ccc"), (8, "d")]);
    h.append(&[(7, "ee")]);
    check!("sizes", h.part_sizes(), vec![4, 1]);
    check!("message 4", h.message(4), Some((7, "ee")));
    check!("message 5", h.message(5), None);
    check!("by author 7", h.by_author(7), vec![0, 2, 4]);
    check!("by an unknown author", h.by_author(9), Vec::<usize>::new());
}

#[test]
fn the_mean_covers_every_message() {
    let mut h = History::new();
    h.append(&[(1, "aaaa"), (1, "aaaa"), (1, "aaaa"), (1, "aaaa")]);
    h.append(&[(1, "aaaaaaaaa")]);
    check!("(4*4 + 9) / 5", h.mean_length(), Some(5.0));
}

#[test]
fn a_snapshot_stays_as_it_was() {
    let mut h = History::new();
    h.append(&[(1, "one")]);
    let old = h.snapshot();
    h.append(&[(1, "two")]);
    check!("the snapshot", (old.len(), old.by_author(1), old.message(1)), (1, vec![0], None));
    check!("the history", (h.len(), h.by_author(1)), (2, vec![0, 1]));
}

#[test]
fn an_empty_batch_changes_nothing() {
    let mut h = History::new();
    h.append(&[(1, "a"), (1, "b")]);
    h.append(&[]);
    check!("sizes", h.part_sizes(), vec![2]);
}
