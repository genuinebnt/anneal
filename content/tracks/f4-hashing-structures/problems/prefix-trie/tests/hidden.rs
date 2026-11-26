use solution::*;

use std::collections::BTreeSet;

#[test]
fn core_the_empty_word() {
    let mut d = Dictionary::new();
    check!("before", (d.contains(""), d.count_prefix(""), d.is_empty()), (false, 0, true));
    check!("insert", (d.insert(""), d.contains(""), d.len()), (true, true, 1));
    check!("remove", (d.remove(""), d.len()), (true, 0));
}

#[test]
fn part3_the_empty_word_is_a_beginning_and_a_completion() {
    let mut d = Dictionary::new();
    d.insert("");
    check!("completions", d.complete("", 5), vec![String::new()]);
    check!("longest", d.longest_stored_prefix("abc"), Some(""));
}

#[test]
fn core_removing_a_missing_word_changes_nothing() {
    let mut d = Dictionary::new();
    d.insert("apple");
    d.insert("apply");
    check!("missing: a prefix, a longer word, a branch", (d.remove("appl"), d.remove("applesauce"), d.remove("apt")), (false, false, false));
    check!("counts", (d.len(), d.count_prefix("app"), d.count_prefix("appl"), d.contains("apple")), (2, 2, 2, true));
}

#[test]
fn core_remove_a_word_that_prefixes_another() {
    let mut d = Dictionary::new();
    d.insert("in");
    d.insert("inn");
    d.remove("in");
    check!("(in, inn, count i)", (d.contains("in"), d.contains("inn"), d.count_prefix("i")), (false, true, 1));
}

#[test]
fn part2_limit_zero_and_exact_word() {
    let mut d = Dictionary::new();
    d.insert("a");
    check!("limit 0", d.complete("", 0), Vec::<String>::new());
    check!("prefix is a word", d.complete("a", 1), vec!["a"]);
}

#[test]
fn part2_unicode_order() {
    let mut d = Dictionary::new();
    for w in ["étude", "zebra", "eel", "éclair", "🦀", "~"] {
        d.insert(w);
    }
    check!("String order", d.complete("", 10), vec!["eel", "zebra", "~", "éclair", "étude", "🦀"]);
    check!("prefix é", d.complete("é", 10), vec!["éclair", "étude"]);
}

#[test]
fn part3_slices_the_text() {
    let mut d = Dictionary::new();
    d.insert("né");
    let text = String::from("née!");
    let found = d.longest_stored_prefix(&text);
    drop(d);
    check!("result outlives the dictionary", found, Some("né"));
}

#[test]
fn part4_chars_not_bytes() {
    let mut d = Dictionary::new();
    d.insert("🦀é");
    check!("two chars, two prefixes", d.prefix_count(), 2);
    d.remove("🦀é");
    check!("none left", d.prefix_count(), 0);
}

#[test]
fn part4_long_word_and_churn() {
    let mut d = Dictionary::new();
    let long = "ab".repeat(1000);
    for _ in 0..50 {
        d.insert(&long);
        d.insert("abc");
        check!("prefixes", d.prefix_count(), 2001);
        d.remove(&long);
        check!("after removing the long word", (d.prefix_count(), d.complete("", 9)), (3, vec!["abc".to_string()]));
        d.remove("abc");
        check!("empty", d.prefix_count(), 0);
    }
}

#[test]
fn part4_random_operations_match_a_sorted_set() {
    let mut rng = anneal_prelude::Rng::new(5401);
    for _ in 0..400 {
        let mut d = Dictionary::new();
        let mut model: BTreeSet<String> = BTreeSet::new();
        let mut trace = Vec::new();
        for _ in 0..40 {
            let len = rng.below(4);
            let word = rng.string(len, "abé");
            if rng.below(3) == 0 {
                trace.push(format!("remove {word:?}"));
                check!(format!("{trace:?}"), d.remove(&word), model.remove(&word));
            } else {
                trace.push(format!("insert {word:?}"));
                check!(format!("{trace:?}"), d.insert(&word), model.insert(word.clone()));
            }
            let len = rng.below(3);
            let prefix = rng.string(len, "abé");
            let limit = rng.below(5);
            let matching: Vec<String> = model.iter().filter(|w| w.starts_with(&prefix)).cloned().collect();
            let want_prefixes: BTreeSet<String> = model.iter().flat_map(|w| w.char_indices().map(|(i, c)| w[..i + c.len_utf8()].to_string())).collect();
            check!(format!("{trace:?}: (len, count_prefix({prefix:?}), prefix_count)"), (d.len(), d.count_prefix(&prefix), d.prefix_count()), (model.len(), matching.len(), want_prefixes.len()));
            check!(format!("{trace:?}: complete({prefix:?}, {limit})"), d.complete(&prefix, limit), matching.into_iter().take(limit).collect::<Vec<_>>());
            check!(format!("{trace:?}: contains({prefix:?})"), d.contains(&prefix), model.contains(&prefix));
            let len = rng.below(6);
            let text = rng.string(len, "abé");
            let want = model.iter().filter(|w| text.starts_with(w.as_str())).max_by_key(|w| w.len()).map(|w| w.len());
            check!(format!("{trace:?}: longest_stored_prefix({text:?})"), d.longest_stored_prefix(&text).map(str::len), want);
        }
    }
}

fn many(n: u32) -> Dictionary {
    let mut d = Dictionary::new();
    for i in 0..n {
        d.insert(&format!("user/{i:06}"));
    }
    d
}

#[test]
fn core_scale_counts_do_not_scan() {
    let n = 100_000;
    let mut d = many(n);
    let mut total = 0;
    for i in 0..50_000 {
        total += d.count_prefix("user/") + d.count_prefix("") + usize::from(d.contains(&format!("user/{i:06}")));
    }
    check!("sum", total, 50_000 * (2 * n as usize + 1));
    for i in (0..n).step_by(2) {
        d.remove(&format!("user/{i:06}"));
    }
    check!("left", d.count_prefix("user/0"), 50_000);
}

#[test]
fn part2_scale_a_limit_stops_the_walk() {
    let d = many(100_000);
    let mut total = 0;
    for _ in 0..20_000 {
        total += d.complete("user/", 3).len();
    }
    check!("results", total, 60_000);
}
