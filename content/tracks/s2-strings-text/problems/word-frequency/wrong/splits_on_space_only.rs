use std::collections::HashMap;
use std::io::{self, BufRead};

pub fn word_frequencies<R: BufRead>(input: R) -> io::Result<Vec<(String, u32)>> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for line in input.lines() {
        let line = line?;
        // Wrong: splits on ' ' only, so a tab inside a line joins two words.
        for raw in line.split(' ') {
            let word = raw.trim_matches(|c: char| !c.is_alphanumeric());
            if !word.is_empty() {
                *counts.entry(word.to_lowercase()).or_insert(0) += 1;
            }
        }
    }
    let mut pairs: Vec<(String, u32)> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(pairs)
}
