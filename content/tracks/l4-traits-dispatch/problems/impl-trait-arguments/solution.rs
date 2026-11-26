use std::fmt::Display;

/// Total number of characters (not bytes) across the words.
pub fn total_chars(words: impl IntoIterator<Item = impl AsRef<str>>) -> usize {
    words.into_iter().map(|w| w.as_ref().chars().count()).sum()
}

/// The items formatted with Display, joined with `sep`.
pub fn join_display(items: impl IntoIterator<Item = impl Display>, sep: &str) -> String {
    items
        .into_iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

/// How many words `keep` accepts.
pub fn count_where(
    words: impl IntoIterator<Item = impl AsRef<str>>,
    keep: impl Fn(&str) -> bool,
) -> usize {
    words.into_iter().filter(|w| keep(w.as_ref())).count()
}

/// `f`, then `g`.
pub fn compose(f: impl Fn(i64) -> i64, g: impl Fn(i64) -> i64) -> impl Fn(i64) -> i64 {
    move |x| g(f(x))
}

/// The even numbers from 0 up to `limit`, inclusive.
pub fn evens(limit: u64) -> impl Iterator<Item = u64> + Clone {
    (0..=limit).step_by(2)
}

/// The items in order, or in reverse when `descending`.
pub fn ordered(v: &[i32], descending: bool) -> Box<dyn Iterator<Item = &i32> + '_> {
    if descending {
        Box::new(v.iter().rev())
    } else {
        Box::new(v.iter())
    }
}
