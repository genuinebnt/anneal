use solution::*;

fn w(word: &str) -> Expr {
    Expr::Word(word.into())
}

fn not(e: Expr) -> Expr {
    Expr::Not(Box::new(e))
}

fn index(pages: &[&str]) -> Index {
    let mut index = Index::new();
    for chunk in pages.chunks(2) {
        index.add_batch(chunk);
    }
    index
}

#[test]
fn an_empty_index() {
    let i = Index::new();
    check!("NOT a", i.matches(&not(w("a"))), Vec::<u32>::new());
    check!("And([])", i.matches(&Expr::And(vec![])), Vec::<u32>::new());
}

#[test]
fn two_nots_cancel() {
    let i = index(&["a", "b", "a b"]);
    check!("NOT NOT a", i.matches(&not(not(w("a")))), vec![0, 2]);
    check!("NOT a NOT b", i.matches(&Expr::And(vec![not(w("a")), not(w("b"))])), Vec::<u32>::new());
    check!("NOT a OR NOT b", i.matches(&Expr::Or(vec![not(w("a")), not(w("b"))])), vec![0, 1]);
}

#[test]
fn each_page_once() {
    let i = index(&["a b c", "c", "b"]);
    check!("a OR b OR c OR a", i.matches(&Expr::Or(vec![w("a"), w("b"), w("c"), w("a")])), vec![0, 1, 2]);
    check!("a a", i.matches(&Expr::And(vec![w("a"), w("a")])), vec![0]);
}

#[test]
fn an_unknown_word() {
    let i = index(&["a", "b"]);
    check!("a zzz", i.matches(&Expr::And(vec![w("a"), w("zzz")])), Vec::<u32>::new());
    check!("a OR zzz", i.matches(&Expr::Or(vec![w("zzz"), w("a")])), vec![0]);
}

#[test]
fn a_phrase_of_one_and_of_none() {
    let i = index(&["x y"]);
    check!("one", i.matches(&Expr::Phrase(vec!["y".into()])), vec![0]);
    check!("none", i.matches(&Expr::Phrase(vec![])), Vec::<u32>::new());
}

#[test]
fn a_very_wide_query() {
    let i = index(&["alpha", "beta", "gamma"]);
    let wide = parse_query(&format!("{}beta", "alpha OR ".repeat(2_000))).expect("flat");
    check!("2001 alternatives", i.matches(&wide), vec![0, 1]);
}

const VOCAB: [&str; 6] = ["cast", "broadcast", "memory", "order", "ordering", "pin"];

fn random(rng: &mut anneal_prelude::Rng, depth: usize) -> Expr {
    let pick = if depth == 0 { rng.below(4) } else { rng.below(8) };
    match pick {
        0 => w(*rng.pick(&VOCAB)),
        1 => Expr::Phrase((0..2).map(|_| rng.pick(&VOCAB).to_string()).collect()),
        2 => Expr::Pattern(rng.pick(&["*cast*", "ord*ring", "*in*", "ry me", "road"]).to_string()),
        3 => {
            if rng.below(4) == 0 {
                Expr::Nothing
            } else {
                w(*rng.pick(&VOCAB))
            }
        }
        4 => not(random(rng, depth - 1)),
        _ => {
            let items = (0..rng.below(4)).map(|_| random(rng, depth - 1)).collect();
            if pick < 7 {
                Expr::And(items)
            } else {
                Expr::Or(items)
            }
        }
    }
}

/// Decides one page at a time, from its text only.
fn fits(e: &Expr, words: &[String], text: &str) -> bool {
    match e {
        Expr::Nothing => false,
        Expr::Word(x) => words.contains(x),
        Expr::Phrase(ws) => words.windows(ws.len()).any(|w| w == ws.as_slice()),
        Expr::Pattern(p) => {
            let mut rest = text;
            p.split('*').all(|piece| match rest.find(piece) {
                Some(at) => {
                    rest = &rest[at + piece.len()..];
                    true
                }
                None => false,
            })
        }
        Expr::And(items) => items.iter().all(|i| fits(i, words, text)),
        Expr::Or(items) => items.iter().any(|i| fits(i, words, text)),
        Expr::Not(inner) => !fits(inner, words, text),
    }
}

#[test]
fn matches_deciding_one_page_at_a_time() {
    let mut rng = anneal_prelude::Rng::new(5207);
    for _ in 0..120 {
        let mut index = Index::new();
        let mut pages: Vec<String> = Vec::new();
        for _ in 0..rng.below(4) {
            let batch: Vec<String> = (0..rng.below(6))
                .map(|_| {
                    let n = rng.below(6);
                    (0..n).map(|_| *rng.pick(&VOCAB)).collect::<Vec<_>>().join(" ")
                })
                .collect();
            let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
            index.add_batch(&refs);
            pages.extend(batch);
        }
        for _ in 0..30 {
            let depth = rng.below(4);
            let query = random(&mut rng, depth);
            let want: Vec<u32> = (0..pages.len())
                .filter(|&p| fits(&query, &tokenize(&pages[p]), &pages[p]))
                .map(|p| p as u32)
                .collect();
            check!(format!("pages {pages:?}, query {query:?}"), index.matches(&query), want);
        }
    }
}

#[test]
fn common_words_over_many_pages() {
    let mut index = Index::new();
    for batch in 0..4 {
        let pages: Vec<&str> = (0..250)
            .map(|i| match (batch, i) {
                (3, 249) => "alpha beta rare",
                (_, i) if i % 2 == 0 => "alpha beta",
                _ => "alpha gamma",
            })
            .collect();
        index.add_batch(&pages);
    }
    // 501 pages have beta, none of them gamma; every page has beta or gamma; 499 lack beta
    for (query, want) in [("alpha beta NOT gamma", 501), ("beta OR gamma", 1_000), ("alpha NOT beta", 499)] {
        check!(query, index.matches(&parse_query(query).expect("good")).len(), want);
    }
}
