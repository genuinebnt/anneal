use solution::*;

fn index(docs: &[(u32, &str)]) -> Index {
    let mut i = Index::new();
    for (id, t) in docs {
        i.add(*id, t);
    }
    i
}

#[test]
fn candidates_are_narrow_because_the_rarest_trigram_decides() {
    let mut docs: Vec<(u32, String)> = (0..500u32).map(|i| (i, format!("common text number {i} with some filler"))).collect();
    docs.push((900, "xx needle yy".to_string()));
    let mut i = Index::new();
    for (id, t) in &docs {
        i.add(*id, t);
    }
    check!("a rare word has one candidate", i.candidates("needle"), Some(vec![900]));
    check!("an unseen trigram settles it", i.candidates("zzz"), Some(vec![]));
    check!("a common word has them all", i.candidates("common").map(|c| c.len()), Some(500));
    check!("search agrees", i.search("needle"), vec![900]);
}

#[test]
fn every_trigram_must_be_present() {
    let i = index(&[(1, "need food"), (2, "feed"), (3, "needle")]);
    check!("only the third has all of nee, eed, edl, dle", i.candidates("needle"), Some(vec![3]));
    check!("two documents have nee and eed", i.candidates("need"), Some(vec![1, 3]));
}

#[test]
fn non_ascii_text_is_indexed_by_char() {
    let i = index(&[(1, "h\u{e9}llo w\u{f6}rld"), (2, "hello world")]);
    check!("two-byte chars", i.search("w\u{f6}rl"), vec![1]);
    check!("a trigram made of multibyte chars", i.candidates("\u{e9}ll"), Some(vec![1]));
    check!("a 2-char query with a multibyte char scans", i.search("\u{e9}l"), vec![1]);
    let j = index(&[(1, "\u{e9}a \u{a9}ab"), (2, "\u{e9}ab")]);
    check!("chars that share bytes are different trigrams", j.candidates("\u{e9}ab"), Some(vec![2]));
    check!("an emoji is one char", index(&[(1, "a\u{1F980}b")]).search("a\u{1F980}b"), vec![1]);
}

#[test]
fn removing_documents() {
    let mut i = index(&[(1, "shared text"), (2, "shared other")]);
    check!("existing", i.remove(1), true);
    check!("again", i.remove(1), false);
    check!("gone from search", i.search("shared"), vec![2]);
    check!("and from the candidates", i.candidates("text"), Some(vec![]));
    check!("never existed", i.remove(77), false);
}

#[test]
fn repeated_trigrams_in_the_query_and_the_document() {
    let i = index(&[(1, "aaaaaa"), (2, "aaa"), (3, "aabaa")]);
    check!("aaaa", i.search("aaaa"), vec![1]);
    check!("aaa", i.search("aaa"), vec![1, 2]);
    check!("a query longer than the document", i.search("aaaaaaa"), Vec::<u32>::new());
}

#[test]
fn matches_a_brute_force_scan_on_random_data() {
    let mut rng = anneal_prelude::Rng::new(321);
    for round in 0..60 {
        let mut i = Index::new();
        let mut docs: Vec<(u32, String)> = Vec::new();
        for id in 0..30u32 {
            let len = rng.below(25);
            let text: String = (0..len).map(|_| ['a', 'b', 'c'][rng.below(3)]).collect();
            i.add(id, &text);
            docs.retain(|(d, _)| *d != id);
            docs.push((id, text));
        }
        if rng.below(3) == 0 {
            let gone = rng.below(30) as u32;
            i.remove(gone);
            docs.retain(|(d, _)| *d != gone);
        }
        docs.sort();
        for _ in 0..40 {
            let qlen = rng.below(6);
            let q: String = (0..qlen).map(|_| ['a', 'b', 'c'][rng.below(3)]).collect();
            let want: Vec<u32> = docs.iter().filter(|(_, t)| t.contains(&q)).map(|(d, _)| *d).collect();
            check!(format!("round {round}, query {q:?}"), i.search(&q), want.clone());
            if let Some(c) = i.candidates(&q) {
                check!(format!("round {round}, query {q:?}: candidates cover every real match"), want.iter().all(|d| c.contains(d)), true);
                check!(format!("candidates are sorted"), c.windows(2).all(|w| w[0] < w[1]), true);
            }
        }
    }
}

#[test]
fn replacing_keeps_the_new_text_searchable() {
    check!(r#"and nothing of the old"#, { let mut i = index(&[(1, "one two three")]); i.add(1, "four five six"); (i.search("two"), i.search("five"), i.candidates("one")) }, (vec![], vec![1], Some(vec![])));
}

#[test]
fn adding_the_same_text_twice_is_idempotent() {
    check!(r#"same id, same text"#, { let mut i = index(&[(1, "repeat me")]); i.add(1, "repeat me"); (i.search("repeat"), i.candidates("me"), i.search("")) }, (vec![1], None, vec![1]));
}

#[test]
fn an_empty_index() {
    check!(r#"no documents"#, { let i = Index::new(); (i.search("abc"), i.search(""), i.candidates("abc")) }, (vec![], vec![], Some(vec![])));
}

#[test]
fn documents_shorter_than_three_chars_have_no_trigrams() {
    check!(r#"found only by scanning"#, { let i = index(&[(1, "ab"), (2, "abc")]); (i.search("ab"), i.search("abc"), i.candidates("abc")) }, (vec![1, 2], vec![2], Some(vec![2])));
}
