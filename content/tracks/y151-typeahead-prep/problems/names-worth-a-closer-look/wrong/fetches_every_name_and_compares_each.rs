use std::collections::{BTreeMap, BTreeSet};

/// The key every name is filed under, for questions in which windows rule nothing out.
const EVERY_NAME: &str = "every name";

/// The distinct runs of three neighbouring places of a text with two empty places added at each end, so that its
/// first and last letters stand in as many windows as the others. `'\0'` stands for an empty place.
fn windows(text: &str) -> BTreeSet<String> {
    let mut places = vec!['\0', '\0'];
    places.extend(text.chars());
    places.extend(['\0', '\0']);
    places.windows(3).map(|w| w.iter().collect()).collect()
}

/// The keys that `name` is filed under.
pub fn keys(name: &str) -> Vec<String> {
    let mut keys: Vec<String> = windows(name).into_iter().collect();
    keys.push(EVERY_NAME.to_string());
    keys
}

/// Every name within `slips` slips of `typed`, closest first, as `(number, slips needed)`.
/// `list(key)` fetches the numbers filed under a key; `compare(number)` is the expensive check.
pub fn near(
    typed: &str,
    slips: usize,
    list: &mut dyn FnMut(&str) -> Vec<usize>,
    compare: &mut dyn FnMut(usize) -> Option<usize>,
) -> Vec<(usize, usize)> {
    let asked = windows(typed);
    // One slip changes at most four windows (a swap does), so a close name keeps all but 4 * slips of them.
    let need = asked.len().saturating_sub(slips.saturating_mul(4));
    let candidates: Vec<usize> = if need < usize::MAX {
        list(EVERY_NAME)
    } else {
        let mut shared: BTreeMap<usize, usize> = BTreeMap::new();
        for window in &asked {
            for number in list(window) {
                *shared.entry(number).or_insert(0) += 1;
            }
        }
        shared
            .into_iter()
            .filter(|&(_, count)| count >= need)
            .map(|(number, _)| number)
            .collect()
    };
    let mut close: Vec<(usize, usize)> = candidates
        .into_iter()
        .filter_map(|n| compare(n).map(|d| (n, d)))
        .collect();
    close.sort_by_key(|&(n, d)| (d, n));
    close
}
