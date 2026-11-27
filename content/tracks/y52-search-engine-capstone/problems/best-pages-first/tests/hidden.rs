use solution::*;
use std::collections::BTreeSet;

fn index(batches: &[&[&str]]) -> Index {
    let mut index = Index::new();
    for batch in batches {
        index.add_batch(batch);
    }
    index
}

const CATS: [&str; 4] = ["rust rust cat", "rust dog cat", "rust dog cat bird fish", "dog cat"];

#[test]
fn two_nots_score_like_none() {
    let i = index(&[&CATS]);
    let plain = i.search("rust", 10);
    check!("NOT NOT rust", i.search("NOT NOT rust", 10), plain.clone());
    check!("scores above 0", plain.map(|h| h.iter().all(|x| x.score > 0.0)), Ok(true));
    // De Morgan: under two NOTs both words count
    check!("NOT (NOT rust AND NOT bird)", i.search("NOT (NOT rust AND NOT bird)", 10), i.search("rust OR bird", 10));
}

#[test]
fn a_word_said_twice_counts_once() {
    let i = index(&[&CATS]);
    let once = i.search("rust", 10);
    check!("rust rust", i.search("rust rust", 10), once.clone());
    check!("rust OR rust", i.search("rust OR rust", 10), once.clone());
}

#[test]
fn phrase_words_score_and_patterns_do_not() {
    let i = index(&[&CATS]);
    let phrase = i.search("\"dog cat\"", 10).expect("good");
    let both = i.search("dog cat", 10).expect("good");
    check!("same pages and scores", phrase, both);
    let pattern = i.search("*ird*", 10).expect("good");
    check!("a pattern", pattern, vec![Hit { page: 2, score: 0.0 }]);
}

#[test]
fn an_empty_index_and_no_match() {
    check!("empty", Index::new().search("a", 5), Ok(vec![]));
    check!("empty, NOT", Index::new().search("NOT a", 5), Ok(vec![]));
    check!("no match", index(&[&CATS]).search("zebra", 5), Ok(vec![]));
    check!("pages with no words", index(&[&["", ""]]).search("NOT a", 5).map(|h| h.len()), Ok(2));
}

#[test]
fn errors_come_through() {
    let i = Index::new();
    check!("quote", i.search("\"", 0), Err(QueryError::UnterminatedQuote(0)));
    check!("deep", i.search(&"(".repeat(100_000), 1), Err(QueryError::TooDeep(50)));
}

#[test]
fn batching_never_changes_a_ranking() {
    let one = index(&[&CATS]);
    let split = index(&[&CATS[..1], &CATS[1..3], &CATS[3..]]);
    for q in ["rust", "dog cat", "rust OR bird", "cat NOT bird", "NOT fish"] {
        check!(q, split.search(q, 10), one.search(q, 10));
    }
}

const VOCAB: [&str; 5] = ["ant", "bee", "cat", "dog", "eel"];

/// A query as text, with the words that should score and the same query as a test on a page.
fn random(rng: &mut anneal_prelude::Rng, depth: usize, negated: bool, scoring: &mut BTreeSet<String>) -> (String, Expr) {
    let pick = if depth == 0 { 0 } else { rng.below(5) };
    match pick {
        0 | 1 => {
            let word = rng.pick(&VOCAB).to_string();
            if !negated {
                scoring.insert(word.clone());
            }
            (word.clone(), Expr::Word(word))
        }
        2 => {
            let (text, e) = random(rng, depth - 1, !negated, scoring);
            (format!("NOT ({text})"), Expr::Not(Box::new(e)))
        }
        _ => {
            let (a, x) = random(rng, depth - 1, negated, scoring);
            let (b, y) = random(rng, depth - 1, negated, scoring);
            if pick == 3 {
                (format!("({a}) ({b})"), Expr::And(vec![x, y]))
            } else {
                (format!("({a}) OR ({b})"), Expr::Or(vec![x, y]))
            }
        }
    }
}

fn fits(e: &Expr, words: &[String]) -> bool {
    match e {
        Expr::Word(x) => words.contains(x),
        Expr::And(items) => items.iter().all(|i| fits(i, words)),
        Expr::Or(items) => items.iter().any(|i| fits(i, words)),
        Expr::Not(inner) => !fits(inner, words),
        _ => unreachable!("not generated"),
    }
}

#[test]
fn matches_scoring_every_page_from_its_text() {
    let mut rng = anneal_prelude::Rng::new(5208);
    for _ in 0..150 {
        let mut index = Index::new();
        let mut pages: Vec<Vec<String>> = Vec::new();
        for _ in 0..rng.below(5) {
            let batch: Vec<String> = (0..rng.below(5))
                .map(|_| {
                    let n = rng.below(8);
                    (0..n).map(|_| *rng.pick(&VOCAB)).collect::<Vec<_>>().join(" ")
                })
                .collect();
            let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
            index.add_batch(&refs);
            pages.extend(batch.iter().map(|p| tokenize(p)));
        }
        let count = pages.len() as f64;
        let mean = pages.iter().map(Vec::len).sum::<usize>() as f64 / count;
        for _ in 0..15 {
            let mut scoring = BTreeSet::new();
            let depth = rng.below(4);
            let (text, tree) = random(&mut rng, depth, false, &mut scoring);
            let score = |page: &[String]| {
                let mut sum = 0.0;
                for t in &scoring {
                    let tf = page.iter().filter(|w| *w == t).count() as f64;
                    if tf > 0.0 {
                        let n = pages.iter().filter(|p| p.contains(t)).count() as f64;
                        let idf = (1.0 + (count - n + 0.5) / (n + 0.5)).ln();
                        sum += idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * page.len() as f64 / mean));
                    }
                }
                sum
            };
            let mut want: Vec<(u32, f64)> = (0..pages.len()).filter(|&p| fits(&tree, &pages[p])).map(|p| (p as u32, score(&pages[p]))).collect();
            let all = want.clone();
            want.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let k = rng.below(6);
            want.truncate(k);
            let got = index.search(&text, k).expect("a good query");
            let context = format!("pages {pages:?}, query {text:?}, k {k}: got {got:?}, want {want:?}");
            check!(format!("{context} (length)"), got.len(), want.len());
            // each hit matches, has its own right score, sits at the score its place calls for, and ties keep page order
            let right = got.iter().zip(&want).all(|(g, w)| {
                all.iter().any(|a| a.0 == g.page && (a.1 - g.score).abs() < 1e-9) && (g.score - w.1).abs() < 1e-9
            });
            let ordered = got.windows(2).all(|w| w[0].score > w[1].score || (w[0].score == w[1].score && w[0].page < w[1].page));
            check!(format!("{context} (scores and order)"), (right, ordered), (true, true));
        }
    }
}

#[test]
fn many_batches_and_common_words() {
    let mut index = Index::new();
    for _ in 0..299 {
        index.add_batch(&["common words here", "common ground"]);
    }
    index.add_batch(&["common common", "x"]);
    check!("ten hits", index.search("common", 10).map(|h| h.len()), Ok(10));
    check!("the double use wins", index.search("common", 10).map(|h| h[0].page), Ok(598));
    check!("with two words", index.search("common words", 10).map(|h| h.len()), Ok(10));
}
