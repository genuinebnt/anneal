use solution::*;
use std::collections::BTreeMap;

/// The suggested terms, joined by commas.
fn names(list: &[Suggestion]) -> String {
    list.iter().map(|s| s.term.as_str()).collect::<Vec<_>>().join(", ")
}

#[test]
fn one_batch_that_moves_several_terms() {
    let mut t = Terms::new();
    t.add(&[("za", 100), ("zb", 90), ("zc", 80), ("zd", 70)], 0);
    t.add(&[("ze", 95), ("zc", 1000)], 0);
    check!("z", names(&t.suggest("z", 20, 0)), "zc, za, ze, zb, zd".to_string());
    check!("empty", names(&t.suggest("", 20, 0)), "zc, za, ze, zb, zd".to_string());
}

#[test]
fn years_of_running_overflow_nothing() {
    let mut t = Terms::new();
    let late = 999_999_000;
    t.add(&[("ab", 5), ("aa", 1), ("ac", 0)], late);
    t.choose("aa", late);
    let got = t.suggest("a", 3, late);
    check!("order", names(&got), "ab, aa, ac".to_string());
    check!("scores", ((got[0].score - 5.0).abs() < 5e-5, (got[1].score - 2.0).abs() < 2e-5, got[2].score), (true, true, 0.0));
}

#[test]
fn forgotten_terms_keep_their_order() {
    let mut t = Terms::new();
    t.add(&[("b", 2), ("a", 1)], 0);
    t.add(&[("c", 1)], 500_000_000);
    t.add(&[("d", 1)], 999_999_999);
    let got = t.suggest("", 4, 999_999_999);
    check!("order", names(&got), "d, c, b, a".to_string());
    check!("tiny scores", got[1..].iter().all(|s| (0.0..=1e-300).contains(&s.score)), true);
}

#[test]
fn never_more_than_twenty() {
    let mut t = Terms::new();
    let words: Vec<String> = (0..60).map(|i| format!("item {i:02}")).collect();
    let batch: Vec<(&str, u32)> = words.iter().enumerate().map(|(i, w)| (w.as_str(), i as u32 + 1)).collect();
    t.add(&batch, 10);
    let top: Vec<String> = (40..60).rev().map(|i| format!("item {i:02}")).collect();
    check!("usize::MAX", names(&t.suggest("ITEM  ", usize::MAX, 10)), top.join(", "));
    check!("zero", t.suggest("item", 0, 10), vec![]);
}

#[test]
fn a_term_below_twenty_climbs_back() {
    let mut t = Terms::new();
    let words: Vec<String> = (0..30).map(|i| format!("k{i:02}")).collect();
    for (i, w) in words.iter().enumerate() {
        t.add(&[(w.as_str(), 100 + i as u32)], 0);
    }
    check!("k00 is not listed", t.suggest("k", 20, 0).iter().any(|s| s.term == "k00"), false);
    t.add(&[("k00", 1000)], 0);
    check!("now first", names(&t.suggest("k", 1, 0)), "k00".to_string());
}

#[test]
fn spellings_and_scripts() {
    let mut t = Terms::new();
    t.add(&[("Crème Brûlée", 3), ("東京タワー", 2), ("İstanbul", 1), ("\u{301}\u{1}", 4)], 0);
    check!("count", t.len(), 3);
    check!("crème", names(&t.suggest(" CREME  B", 5, 0)), "Crème Brûlée".to_string());
    check!("東京", names(&t.suggest("東京", 5, 0)), "東京タワー".to_string());
    check!("istanbul", names(&t.suggest("istanbul", 5, 0)), "İstanbul".to_string());
    check!("a mark alone is no term", t.choose("\u{301}", 0), false);
}

#[test]
fn an_earlier_write_is_still_old() {
    let mut t = Terms::new();
    t.add(&[("a", 1)], 7200);
    t.add(&[("a", 4)], 0);
    check!("1 + 4/4", (t.suggest("a", 1, 7200)[0].score - 2.0).abs() < 2e-5, true);
}

type History = Vec<(u32, u64)>;

fn exact(h: &History, now: u64) -> f64 {
    h.iter().map(|&(c, at)| f64::from(c) * (-((now - at) as f64) / 3600.0).exp2()).sum()
}

/// The keys of the generated texts need only lowercasing, `é` and spaces.
fn plain_key(text: &str) -> String {
    text.to_lowercase().replace('é', "e").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn agrees_with_adding_up_every_choice() {
    let mut rng = anneal_prelude::Rng::new(15201);
    let letters = ['a', 'b', 'A', 'é', 'e', ' '];
    for round in 0..250 {
        let mut t = Terms::new();
        let mut model: BTreeMap<String, (String, History)> = BTreeMap::new();
        let base = [0u64, 40_000_000, 999_000_000][round % 3];
        let mut latest = base;
        for _ in 0..rng.below(25) {
            let at = base + 600 * rng.below(60) as u64;
            latest = latest.max(at);
            let n = rng.below(6);
            let batch: Vec<(String, u32)> = (0..n)
                .map(|_| {
                    let len = rng.below(5);
                    let text: String = (0..len).map(|_| *rng.pick(&letters)).collect();
                    (text, rng.below(4) as u32)
                })
                .collect();
            let pairs: Vec<(&str, u32)> = batch.iter().map(|(s, c)| (s.as_str(), *c)).collect();
            let mut want = Added { added: 0, existing: 0 };
            for (text, count) in &batch {
                let key = plain_key(text);
                if key.is_empty() {
                    continue;
                }
                if model.contains_key(&key) {
                    want.existing += 1;
                } else {
                    want.added += 1;
                    model.insert(key.clone(), (text.trim().to_string(), Vec::new()));
                }
                if *count > 0 {
                    model.get_mut(&key).unwrap().1.push((*count, at));
                }
            }
            check!(format!("add {batch:?}"), t.add(&pairs, at), want);
        }
        let now = latest + rng.below(5000) as u64;
        check!("len", t.len(), model.len());
        for _ in 0..12 {
            let len = rng.below(3);
            let typed: String = (0..len).map(|_| *rng.pick(&letters)).collect();
            let limit = rng.below(25);
            let got = t.suggest(&typed, limit, now);
            let key = plain_key(&typed);
            let fits: Vec<(&String, &(String, History))> = model.iter().filter(|(k, _)| k.starts_with(&key)).collect();
            let label = format!("suggest({typed:?}, {limit}) -> {got:?}");
            check!(format!("{label}: how many"), got.len(), fits.len().min(limit).min(20));
            let found: Vec<(&String, &History)> = got
                .iter()
                .map(|s| {
                    let k = plain_key(&s.term);
                    let (k, (shown, h)) = model.get_key_value(&k).expect("an unknown term was suggested");
                    assert_eq!(&s.term, shown, "not the first spelling");
                    assert!(k.starts_with(&key), "{k:?} does not start with {key:?}");
                    let want = exact(h, now);
                    assert!((s.score - want).abs() <= want * 1e-5, "score of {k:?}: {} but {want}", s.score);
                    (k, h)
                })
                .collect();
            // ordering is judged at the latest choice, where larger scores are far from underflow
            let before = |a: (&String, &History), b: (&String, &History)| {
                let (x, y) = (exact(a.1, latest), exact(b.1, latest));
                if a.1 == b.1 { a.0 < b.0 } else { y <= x * (1.0 + 1e-5) }
            };
            for pair in found.windows(2) {
                check!(format!("{label}: {} before {}", pair[0].0, pair[1].0), before(pair[0], pair[1]), true);
            }
            if let Some(&last) = found.last() {
                for (k, (_, h)) in &fits {
                    if found.iter().all(|f| f.0 != *k) {
                        check!(format!("{label}: {k:?} was left out"), before(last, (*k, h)), true);
                    }
                }
            }
        }
    }
}
