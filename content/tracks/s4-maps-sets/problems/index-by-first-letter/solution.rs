use std::collections::BTreeMap;

pub fn index(words: &[&str]) -> BTreeMap<char, Vec<String>> {
    let mut idx: BTreeMap<char, Vec<String>> = BTreeMap::new();
    for w in words {
        if let Some(first) = w.chars().next() {
            idx.entry(first.to_ascii_lowercase())
                .or_default()
                .push(w.to_string());
        }
    }
    for list in idx.values_mut() {
        list.sort();
    }
    idx
}
