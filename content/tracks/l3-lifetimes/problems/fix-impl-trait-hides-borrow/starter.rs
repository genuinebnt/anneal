/// Words of `text` longer than `min` chars, as owned `String`s.
pub fn long_words(text: &str, min: usize) -> impl Iterator<Item = String> {
    text.split_whitespace()
        .filter(move |w| w.chars().count() > min)
        .map(String::from)
}

/// Pairs up `a` and `b` element by element (stopping at the shorter one).
pub fn pairs<'a, 'b>(a: &'a [u32], b: &'b [u32]) -> impl Iterator<Item = (u32, u32)> + 'a + 'b {
    a.iter().copied().zip(b.iter().copied())
}

/// The values of `v`, sorted, as a snapshot: later changes to `v` don't affect it.
pub fn sorted_snapshot(v: &Vec<u32>) -> impl Iterator<Item = u32> + '_ {
    let mut copy = v.to_vec();
    copy.sort_unstable();
    copy.into_iter()
}
