pub fn sizes(s: &str) -> (usize, usize) {
    (s.len(), s.chars().count())
}

pub fn positions(s: &str, target: char) -> Vec<usize> {
    s.char_indices()
        .filter(|&(_, c)| c == target)
        .map(|(i, _)| i)
        .collect()
}

pub fn nth_char(s: &str, n: usize) -> Option<char> {
    s.chars().nth(n)
}
