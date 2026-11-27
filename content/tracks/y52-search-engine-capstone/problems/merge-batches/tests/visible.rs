use solution::*;

fn index(batches: &[&[&str]]) -> Index {
    let mut index = Index::new();
    for batch in batches {
        index.add_batch(batch);
    }
    index
}

const A: [&str; 2] = ["memory ordering", "async rust"];
const B: [&str; 1] = ["rust memory"];
const C: [&str; 2] = ["broadcast", "memory ordering again"];

#[test]
fn two_neighbours_become_one() {
    let mut i = index(&[&A, &B, &C]);
    check!("merged", (i.merge(1, 2), i.batches(), i.len()), (true, 2, 5));
    let fresh = index(&[&A, &["rust memory", "broadcast", "memory ordering again"]]);
    check!("packed bytes", i.packed_bytes(), fresh.packed_bytes());
    check!("postings of memory", i.postings("memory"), fresh.postings("memory"));
}

#[test]
fn answers_stay_the_same() {
    let mut i = index(&[&A, &B, &C]);
    let before = (i.search("memory OR rust", 10), i.find_pattern("*cast*"), i.phrase(&["memory", "ordering"]));
    i.merge_all();
    check!("one batch", i.batches(), 1);
    let after = (i.search("memory OR rust", 10), i.find_pattern("*cast*"), i.phrase(&["memory", "ordering"]));
    check!("search, pattern, phrase", after, before);
}

#[test]
fn ranges_that_cannot_be_merged() {
    let mut i = index(&[&A, &B]);
    for (first, count) in [(0, 0), (0, 1), (0, 3), (1, 2), (2, 2), (usize::MAX, 2), (1, usize::MAX)] {
        check!(format!("merge({first}, {count})"), i.merge(first, count), false);
    }
    check!("unchanged", i.batches(), 2);
}

#[test]
fn merge_all_of_nothing_and_of_one() {
    let mut i = Index::new();
    i.merge_all();
    check!("none", i.batches(), 0);
    i.add_batch(&A);
    i.merge_all();
    check!("one", (i.batches(), i.len()), (1, 2));
}

#[test]
fn adding_after_a_merge() {
    let mut i = index(&[&A, &B]);
    i.merge_all();
    i.add_batch(&C);
    check!("pages", (i.batches(), i.len(), i.phrase(&["memory", "ordering"])), (2, 5, vec![0, 4]));
}
