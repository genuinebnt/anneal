//! Common tasks with `HashMap::entry`. Each function is one look-up answer.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::num::ParseIntError;

/// Count: one counter per distinct word.
pub fn count_words(text: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts
}

/// Group: one `Vec` per key, created on first use.
pub fn group_by_len<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
    let mut groups: HashMap<usize, Vec<&str>> = HashMap::new();
    for &word in words {
        groups.entry(word.len()).or_default().push(word);
    }
    groups
}

/// Update if present, otherwise insert: keep the best score per name.
pub fn record_best<'a>(best: &mut HashMap<&'a str, i32>, name: &'a str, score: i32) {
    best.entry(name)
        .and_modify(|old| *old = (*old).max(score))
        .or_insert(score);
}

/// Different logic per case: take one item, and drop the key when none are left.
/// `None` means the item was not in stock. Counts of zero are never stored.
pub fn take_one(stock: &mut HashMap<String, u32>, item: String) -> Option<u32> {
    match stock.entry(item) {
        Entry::Occupied(mut slot) => {
            let left = slot.get().saturating_sub(1);
            if left == 0 {
                slot.remove();
            } else {
                *slot.get_mut() = left;
            }
            Some(left)
        }
        Entry::Vacant(_) => None,
    }
}

/// The value is computed from the key, so the key does not need a clone.
pub fn remember_len(cache: &mut HashMap<String, usize>, key: String) -> usize {
    *cache.entry(key).or_insert_with_key(|k| k.chars().count())
}

/// Nested maps: user -> event -> count.
pub fn tally<'a>(
    by_user: &mut HashMap<&'a str, HashMap<&'a str, u32>>,
    user: &'a str,
    event: &'a str,
) {
    *by_user.entry(user).or_default().entry(event).or_insert(0) += 1;
}

/// `String` keys, borrowed input, mostly hits: allocate the key only on a miss.
pub fn bump(counts: &mut HashMap<String, u32>, word: &str) {
    if let Some(n) = counts.get_mut(word) {
        *n += 1;
    } else {
        counts.insert(word.to_owned(), 1);
    }
}

/// The same job with `entry`: shorter, but builds a `String` on every call.
pub fn bump_with_entry(counts: &mut HashMap<String, u32>, word: &str) {
    *counts.entry(word.to_owned()).or_insert(0) += 1;
}

/// Return a reference and still allocate only on a miss.
/// The obvious "return early from `get_mut`" version does not compile (see the page).
pub fn counter<'m>(counts: &'m mut HashMap<String, u32>, word: &str) -> &'m mut u32 {
    if !counts.contains_key(word) {
        counts.insert(word.to_owned(), 0);
    }
    counts
        .get_mut(word)
        .expect("present: inserted above if it was missing")
}

/// Fallible construction. `or_try_insert_with` is unstable, so match instead.
/// On error nothing is inserted.
pub fn parsed(cache: &mut HashMap<String, u64>, text: String) -> Result<&mut u64, ParseIntError> {
    match cache.entry(text) {
        Entry::Occupied(slot) => Ok(slot.into_mut()),
        Entry::Vacant(slot) => {
            let value = slot.key().parse()?;
            Ok(slot.insert(value))
        }
    }
}

/// Recursive memo. The closure given to `or_insert_with` cannot use the map,
/// so look up first, compute with the map free, then insert.
pub fn collatz_len(n: u64, memo: &mut HashMap<u64, u32>) -> u32 {
    if n == 1 {
        return 1;
    }
    if let Some(&len) = memo.get(&n) {
        return len;
    }
    let next = if n.is_multiple_of(2) {
        n / 2
    } else {
        3 * n + 1
    };
    let len = 1 + collatz_len(next, memo);
    memo.insert(n, len);
    len
}
