use solution::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};

// ---- the rule, written without shortcuts ----

fn table(a: &[char], b: &[char]) -> usize {
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        for j in 0..=b.len() {
            d[i][j] = if i == 0 || j == 0 {
                i + j
            } else {
                let mut best = (d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1])).min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
                if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                    best = best.min(d[i - 2][j - 2] + 1);
                }
                best
            };
        }
    }
    d[a.len()][b.len()]
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn padded(key: &str) -> BTreeSet<String> {
    // '#' is not used by any generated key
    chars(&format!("##{key}##")).windows(3).map(|w| w.iter().collect()).collect()
}

fn shared_and_needed(typed: &str, term: &str) -> (usize, usize) {
    let allowed = if typed.chars().count() <= 5 { 1 } else { 2 };
    let mine = padded(typed);
    let shared = mine.intersection(&padded(term)).count();
    (shared, mine.len().saturating_sub(4 * allowed).max(2))
}

fn rule(typed: &str, term: &str) -> Option<(Kind, u8)> {
    if term.starts_with(typed) {
        return Some((Kind::Prefix, 0));
    }
    let (q, t) = (chars(typed), chars(term));
    if q.len() < 3 {
        return None;
    }
    if term.contains(typed) {
        return Some((Kind::Infix, 0));
    }
    let allowed = if q.len() <= 5 { 1 } else { 2 };
    let (shared, needed) = shared_and_needed(typed, term);
    if shared < needed {
        return None;
    }
    let mut dist = table(&q, &t);
    if t.len() > q.len() {
        dist = dist.min(table(&q, &t[..q.len()]));
    }
    (dist <= allowed).then_some((Kind::Typo, dist as u8))
}

fn could_match(typed: &str, term: &str) -> bool {
    let q = chars(typed);
    let every_inner = q.windows(3).all(|w| term.contains(&w.iter().collect::<String>()));
    let (shared, needed) = shared_and_needed(typed, term);
    every_inner || shared >= needed
}

fn word(rng: &mut anneal_prelude::Rng, letters: &[char], min: usize, max: usize) -> String {
    let n = min + rng.below(max - min + 1);
    (0..n).map(|_| *rng.pick(letters)).collect()
}

fn near_word(rng: &mut anneal_prelude::Rng, from: &str, letters: &[char]) -> String {
    let mut w = chars(from);
    let start = rng.below(w.len().max(1));
    if rng.bool() {
        w = w.split_off(start.min(w.len()));
    }
    w.truncate(1 + rng.below(10));
    for _ in 0..rng.below(3) {
        match rng.below(4) {
            0 => w.insert(rng.below(w.len() + 1), *rng.pick(letters)),
            1 if !w.is_empty() => {
                w.remove(rng.below(w.len()));
            }
            2 if !w.is_empty() => {
                let at = rng.below(w.len());
                w[at] = *rng.pick(letters);
            }
            _ if w.len() > 1 => {
                let at = rng.below(w.len() - 1);
                w.swap(at, at + 1);
            }
            _ => {}
        }
    }
    w.into_iter().collect()
}

#[test]
fn the_rule_agrees_with_the_statement() {
    let mut rng = anneal_prelude::Rng::new(15202);
    let letters = ['a', 'b', 'c', 'é'];
    for _ in 0..4000 {
        let term = word(&mut rng, &letters, 0, 12);
        let typed = if rng.below(4) == 0 { word(&mut rng, &letters, 0, 9) } else { near_word(&mut rng, &term, &letters) };
        check!(format!("classify({typed:?}, {term:?})"), classify(&typed, &term), rule(&typed, &term));
    }
}

#[test]
fn the_second_test_has_a_floor_of_two() {
    check!("abc / abd shares ##a #ab", classify("abc", "abd"), Some((Kind::Typo, 1)));
    check!("abc / xbc shares bc# c##", classify("abc", "xbc"), Some((Kind::Typo, 1)));
    check!("abc / axcq shares only ##a", classify("abc", "axcq"), None);
    check!("repeated letters", classify("aaaaaaaa", "aaaaabaaa"), rule("aaaaaaaa", "aaaaabaaa"));
}

#[test]
fn a_half_typed_word_with_a_slip() {
    check!("progrma", classify("progrma", "programming"), Some((Kind::Typo, 1)));
    check!("two slips in six", classify("pragrm", "programming"), rule("pragrm", "programming"));
}

#[test]
fn a_judge_that_says_no_leaves_only_beginnings() {
    let mut i = Index::new();
    i.add(&[("javascript", 5), ("scripture", 1), ("typescript", 2)], 0);
    let got = i.search_with("script", 10, 0, &mut |_| None);
    check!("only the prefix", got.iter().map(|s| s.term.as_str()).collect::<Vec<_>>(), vec!["scripture"]);
}

#[test]
fn a_full_answer_needs_no_judge() {
    let mut i = Index::new();
    i.add(&[("scripture", 1), ("script", 2), ("javascript", 50)], 0);
    let mut calls = 0;
    let got = i.search_with("script", 2, 0, &mut |_| {
        calls += 1;
        None
    });
    check!("answer and calls", (got.len(), calls), (2, 0));
    let nothing = i.search_with("script", 0, 0, &mut |_| {
        calls += 1;
        None
    });
    i.search_with("sc", 20, 0, &mut |_| {
        calls += 1;
        None
    });
    check!("limit 0 and two characters", (nothing.len(), calls), (0, 0));
}

#[test]
fn infix_before_typo_however_popular() {
    let mut i = Index::new();
    i.add(&[("sprint", 900), ("blueprint", 1), ("print", 3), ("pint", 50)], 0);
    let got: Vec<(String, u8)> = i.search("print", 9, 0).into_iter().map(|s| (s.term, s.dist)).collect();
    let want = [("print", 0), ("sprint", 0), ("blueprint", 0), ("pint", 1)];
    check!("print", got, want.iter().map(|&(t, d)| (t.to_string(), d)).collect::<Vec<_>>());
}

#[test]
fn writes_still_work() {
    let mut i = Index::new();
    i.add(&[("ab1", 100), ("ab2", 90), ("ab3", 80)], 0);
    i.add(&[("ab4", 95), ("ab3", 1000)], 999_999_000);
    check!("order", i.search("ab", 9, 999_999_000).iter().map(|s| s.term.clone()).collect::<Vec<_>>(), ["ab3", "ab4", "ab1", "ab2"].iter().map(|s| s.to_string()).collect::<Vec<_>>());
    check!("choose", (i.choose("AB2", 999_999_000), i.choose("zz", 0)), (true, false));
}

/// Random terms with distinct whole-number scores, so that the expected answer is exact.
#[test]
fn agrees_with_judging_every_term_and_asks_only_about_possible_ones() {
    let mut rng = anneal_prelude::Rng::new(15203);
    let letters = ['a', 'b', 'c', 'd', 'e', 'é', 'r', 's'];
    for round in 0..40 {
        let mut i = Index::new();
        // key -> count; `é` folds to `e`
        let mut terms: BTreeMap<String, u32> = BTreeMap::new();
        let size = if round == 0 { 600 } else { rng.below(80) };
        for n in 0..size {
            let text = word(&mut rng, &letters, 1, 10);
            let key = text.replace('é', "e");
            if let std::collections::btree_map::Entry::Vacant(slot) = terms.entry(key) {
                let count = 1000 + 3 * n as u32 + rng.below(3) as u32;
                slot.insert(count);
                i.add(&[(text.as_str(), count)], 0);
            }
        }
        let keys: Vec<String> = terms.keys().cloned().collect();
        for _ in 0..25 {
            let typed = if keys.is_empty() || rng.below(5) == 0 {
                word(&mut rng, &letters, 0, 8)
            } else {
                let from = keys[rng.below(keys.len())].clone();
                near_word(&mut rng, &from, &letters)
            }
            .replace('é', "e");
            let limit = 1 + rng.below(25);
            let mut want: Vec<(u8, u8, u32, &String, Kind)> = terms
                .iter()
                .filter_map(|(k, &c)| rule(&typed, k).map(|(kind, d)| (u8::from(kind != Kind::Prefix), d, u32::MAX - c, k, kind)))
                .collect();
            want.sort_by_key(|w| (w.0, w.1, w.2));
            want.truncate(limit.min(20));
            let prefixes = keys.iter().filter(|k| k.starts_with(&typed)).count();
            let mut asked: Vec<String> = Vec::new();
            let got = i.search_with(&typed, limit, 0, &mut |key| {
                asked.push(key.to_string());
                rule(&typed, key)
            });
            let label = format!("search({typed:?}, {limit}) over {} terms", terms.len());
            let got: Vec<(String, Kind, u8)> = got.into_iter().map(|s| (s.term.replace('é', "e"), s.kind, s.dist)).collect();
            let want: Vec<(String, Kind, u8)> = want.into_iter().map(|w| (w.3.clone(), w.4, w.1)).collect();
            check!(label.clone(), got, want.clone());
            let same: Vec<(String, Kind, u8)> = i.search(&typed, limit, 0).into_iter().map(|s| (s.term.replace('é', "e"), s.kind, s.dist)).collect();
            check!(format!("{label}, with your own classify"), same, want);
            if prefixes >= limit.min(20) || typed.chars().count() < 3 {
                check!(format!("{label}: calls that should not happen"), asked.len(), 0);
            }
            let mut once = HashSet::new();
            for key in &asked {
                check!(format!("{label}: asked twice about {key:?}"), once.insert(key.clone()), true);
                check!(format!("{label}: asked about {key:?}, which starts with the typed key"), key.starts_with(&typed), false);
                check!(format!("{label}: asked about {key:?}, which could not match"), could_match(&typed, key), true);
            }
        }
    }
}
