use std::collections::{HashMap, HashSet};

pub fn most_common_word(paragraph: &str, banned: &[&str]) -> String {
    let banned: HashSet<&str> = banned.iter().copied().collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for w in paragraph
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
    {
        let w = w.to_ascii_lowercase();
        if !banned.contains(w.as_str()) {
            *counts.entry(w).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(w, _)| w)
        .unwrap_or_default()
}
