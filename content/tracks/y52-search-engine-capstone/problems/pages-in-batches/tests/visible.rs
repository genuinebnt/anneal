use solution::*;

fn index(batches: &[&[&str]]) -> Index {
    let mut index = Index::new();
    for batch in batches {
        index.add_batch(batch);
    }
    index
}

fn posting(page: u32, positions: &[u32]) -> Posting {
    Posting {
        page,
        positions: positions.to_vec(),
    }
}

#[test]
fn an_empty_index() {
    let i = Index::new();
    check!("counts", (i.len(), i.batches(), i.packed_bytes()), (0, 0, 0));
    check!("questions", (i.page_len(0), i.postings("a"), i.phrase(&["a"])), (None, vec![], vec![]));
}

#[test]
fn pages_are_numbered_across_batches() {
    let i = index(&[&["the cat", "a dog"], &["The CAT sat on the cat"]]);
    check!("counts", (i.len(), i.batches()), (3, 2));
    check!("lengths", (i.page_len(0), i.page_len(2), i.page_len(3)), (Some(2), Some(6), None));
    check!("postings of cat", i.postings("cat"), vec![posting(0, &[1]), posting(2, &[1, 5])]);
}

#[test]
fn phrases() {
    let i = index(&[&["memory ordering matters", "ordering memory"], &["shared memory-ordering"]]);
    check!("memory ordering", i.phrase(&["memory", "ordering"]), vec![0, 2]);
    check!("ordering memory", i.phrase(&["ordering", "memory"]), vec![1]);
    check!("one word", i.phrase(&["shared"]), vec![2]);
    check!("no words", i.phrase(&[]), Vec::<u32>::new());
}

#[test]
fn an_empty_batch_changes_nothing() {
    let mut i = index(&[&["x"]]);
    i.add_batch(&[]);
    check!("counts", (i.len(), i.batches()), (1, 1));
}

#[test]
fn packed_size() {
    // one batch, one word: [1 posting, page 0, 2 positions, gaps 0 and 1]
    let i = index(&[&["go go"]]);
    check!("go go", i.packed_bytes(), 5);
}
