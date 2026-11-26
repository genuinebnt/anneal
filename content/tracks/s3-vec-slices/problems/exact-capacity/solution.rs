pub fn join_with(parts: &[&str], sep: &str) -> String {
    let total =
        parts.iter().map(|p| p.len()).sum::<usize>() + sep.len() * parts.len().saturating_sub(1);
    let mut out = String::with_capacity(total);
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push_str(p);
    }
    out
}
