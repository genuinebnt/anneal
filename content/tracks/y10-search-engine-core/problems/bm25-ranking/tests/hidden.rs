use solution::*;

fn index(docs: &[(u32, &str)]) -> Index {
    let mut i = Index::new();
    for (id, t) in docs {
        i.add(*id, t);
    }
    i
}

fn ids(r: &[(u32, f64)]) -> Vec<u32> {
    r.iter().map(|x| x.0).collect()
}

/// A from-scratch scorer: the oracle.
fn reference(docs: &[(u32, String)], query: &str, k: usize) -> Vec<(u32, f64)> {
    let toks: Vec<(u32, Vec<String>)> = docs.iter().map(|(id, t)| (*id, tokenize(t))).collect();
    let n = toks.len() as f64;
    if toks.is_empty() {
        return vec![];
    }
    let avg = toks.iter().map(|(_, t)| t.len()).sum::<usize>() as f64 / n;
    let mut qt: Vec<String> = Vec::new();
    for t in tokenize(query) {
        if !qt.contains(&t) {
            qt.push(t);
        }
    }
    let mut out = Vec::new();
    for (id, t) in &toks {
        let mut score = 0.0;
        let mut hit = false;
        for q in &qt {
            let nt = toks.iter().filter(|(_, d)| d.contains(q)).count() as f64;
            let tf = t.iter().filter(|w| *w == q).count() as f64;
            if tf > 0.0 {
                hit = true;
                let idf = (1.0 + (n - nt + 0.5) / (nt + 0.5)).ln();
                score += idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * t.len() as f64 / avg));
            }
        }
        if hit {
            out.push((*id, score));
        }
    }
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    out.truncate(k);
    out
}

#[test]
fn matches_a_from_scratch_scorer_on_random_corpora() {
    let mut rng = anneal_prelude::Rng::new(2718);
    let vocab = ["rust", "go", "zig", "c", "java", "swift", "kotlin", "ruby", "lua", "perl"];
    for round in 0..80 {
        let ndocs = rng.below(25) + 1;
        let mut docs: Vec<(u32, String)> = Vec::new();
        let mut idx = Index::new();
        for id in 0..ndocs as u32 {
            let len = rng.below(12);
            let text = (0..len).map(|_| vocab[rng.below(vocab.len())]).collect::<Vec<_>>().join(" ");
            idx.add(id, &text);
            docs.push((id, text));
        }
        for _ in 0..10 {
            let qn = rng.below(4) + 1;
            let query = (0..qn).map(|_| vocab[rng.below(vocab.len())]).collect::<Vec<_>>().join(" ");
            let k = rng.below(8) + 1;
            let got = idx.search(&query, k);
            let want = reference(&docs, &query, k);
            check!(format!("round {round} {query:?}: ids"), ids(&got), ids(&want));
            for (g, w) in got.iter().zip(&want) {
                check!(format!("round {round} {query:?}: score of {}", g.0), (g.1 - w.1).abs() < 1e-9, true);
            }
        }
    }
}

#[test]
fn replacing_a_document_updates_every_statistic() {
    let mut i = index(&[(1, "apple banana"), (2, "banana cherry")]);
    i.add(1, "durian");
    check!("old terms no longer match", ids(&i.search("apple", 5)), Vec::<u32>::new());
    let docs = vec![(1u32, "durian".to_string()), (2u32, "banana cherry".to_string())];
    let want = reference(&docs, "banana durian", 5);
    let got = i.search("banana durian", 5);
    check!("ids", ids(&got), ids(&want));
    check!("scores use the new lengths and counts", got.iter().zip(&want).all(|(g, w)| (g.1 - w.1).abs() < 1e-9), true);
}

#[test]
fn repeated_query_terms_count_once() {
    check!(r#"rust rust = rust"#, { let i = index(&[(1, "rust code"), (2, "code only")]); let a = i.search("rust", 5); let b = i.search("rust rust RUST", 5); a.len() == b.len() && (a[0].1 - b[0].1).abs() < 1e-12 }, true);
}

#[test]
fn ties_are_broken_by_id() {
    check!(r#"identical documents"#, { let i = index(&[(7, "same text"), (3, "same text"), (5, "same text")]); ids(&i.search("same", 10)) }, vec![3, 5, 7]);
}

#[test]
fn longer_documents_score_lower() {
    check!(r#"same tf, more words"#, { let i = index(&[(1, "rust a b c d e f g h i j"), (2, "rust")]); ids(&i.search("rust", 5)) }, vec![2, 1]);
}

#[test]
fn term_frequency_saturates() {
    check!(r#"20 occurrences are not 10x one"#, { let many = "rust ".repeat(20); let i = index(&[(1, many.trim()), (2, "rust x x x x x x x x x x x x x x x x x x x")]); let r = i.search("rust", 5); (r[0].0, r[0].1 / r[1].1 < 3.0) }, (1, true));
}

#[test]
fn scores_are_positive_even_for_common_terms() {
    check!(r#"idf uses ln(1 + ...)"#, { let i = index(&[(1, "the cat"), (2, "the dog"), (3, "the bird")]); i.search("the", 5).iter().all(|r| r.1 > 0.0) }, true);
}

#[test]
fn empty_things() {
    check!(r#"no index, no query, k = 0"#, { let e = Index::new(); let i = index(&[(1, "a b c")]); (e.search("a", 5).len(), i.search("", 5).len(), i.search("!!!", 5).len(), i.search("a", 0).len(), i.search("zzz", 5).len()) }, (0, 0, 0, 0, 0));
}

#[test]
fn unicode_terms() {
    check!(r#"case folding and non-ASCII letters"#, { let i = index(&[(1, "\u{c9}COLE \u{e9}cole"), (2, "college")]); ids(&i.search("\u{e9}cole", 5)) }, vec![1]);
}

#[test]
fn numbers_are_terms() {
    check!(r#"alphanumeric"#, { let i = index(&[(1, "error 404 not found"), (2, "error 500")]); ids(&i.search("404", 5)) }, vec![1]);
}
