pub fn join_words(words: &[&str], sep: &str) -> String {
    let len =
        words.iter().map(|w| w.len()).sum::<usize>() + sep.len() * words.len().saturating_sub(1);
    let mut out = String::with_capacity(len);
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push_str(w);
    }
    out
}
