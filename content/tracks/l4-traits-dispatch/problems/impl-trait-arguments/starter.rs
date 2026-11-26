use std::fmt::Display;

/// Total number of characters (not bytes) across the words.
pub fn total_chars(words: &[String]) -> usize {
    words.iter().map(|w| w.chars().count()).sum()
}

/// The items formatted with Display, joined with `sep`.
pub fn join_display(items: &[i32], sep: &str) -> String {
    items
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

/// How many words `keep` accepts.
pub fn count_where(words: &[String], keep: fn(&str) -> bool) -> usize {
    words.iter().filter(|w| keep(w)).count()
}

/// `f`, then `g`.
pub fn compose(f: fn(i64) -> i64, g: fn(i64) -> i64) -> Box<dyn Fn(i64) -> i64> {
    Box::new(move |x| g(f(x)))
}

/// The even numbers from 0 up to `limit`, inclusive.
pub fn evens(limit: u64) -> std::vec::IntoIter<u64> {
    (0..=limit)
        .filter(|n| n % 2 == 0)
        .collect::<Vec<_>>()
        .into_iter()
}

/// The items in order, or in reverse when `descending`.
pub fn ordered(v: &[i32], descending: bool) -> impl Iterator<Item = &i32> {
    if descending {
        v.iter().rev()
    } else {
        v.iter()
    }
}
