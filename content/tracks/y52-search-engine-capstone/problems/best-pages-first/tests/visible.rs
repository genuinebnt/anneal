use solution::*;

fn index(pages: &[&str]) -> Index {
    let mut index = Index::new();
    index.add_batch(pages);
    index
}

fn pages(hits: Result<Vec<Hit>, QueryError>) -> Vec<u32> {
    hits.expect("a good query").iter().map(|h| h.page).collect()
}

const CATS: [&str; 4] = ["rust rust cat", "rust dog cat", "rust dog cat bird fish", "dog cat"];

#[test]
fn more_uses_and_shorter_pages_rank_higher() {
    check!("rust", pages(index(&CATS).search("rust", 10)), vec![0, 1, 2]);
}

#[test]
fn the_score_of_a_lone_page() {
    let hits = index(&["rust"]).search("rust", 5).expect("good");
    check!("one hit", hits.len(), 1);
    check!("ln(1 + 0.5/1.5)", (hits[0].score - 0.28768207245178096).abs() < 1e-9, true);
}

#[test]
fn k_limits_the_answer() {
    let i = index(&CATS);
    check!("top 2", pages(i.search("rust", 2)), vec![0, 1]);
    check!("top 0", pages(i.search("rust", 0)), vec![]);
    check!("all of them", pages(i.search("cat", usize::MAX)).len(), 4);
}

#[test]
fn equal_scores_keep_page_order() {
    check!("x", pages(index(&["x y", "z", "x y", "y x"]).search("x", 10)), vec![0, 2, 3]);
}

#[test]
fn words_under_not_do_not_score() {
    let hits = index(&CATS).search("NOT bird", 10).expect("good");
    check!(
        "matching pages, score 0",
        hits.iter().map(|h| (h.page, h.score)).collect::<Vec<_>>(),
        vec![(0, 0.0), (1, 0.0), (3, 0.0)]
    );
}

#[test]
fn a_bad_query() {
    check!("(rust", index(&CATS).search("(rust", 3), Err(QueryError::UnexpectedEnd));
}
