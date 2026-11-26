use std::collections::HashMap;

pub fn report(scores: &HashMap<String, u32>) -> Vec<String> {
    let mut rows: Vec<(&String, &u32)> = scores.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    rows.into_iter()
        .map(|(name, score)| format!("{name}: {score}"))
        .collect()
}
