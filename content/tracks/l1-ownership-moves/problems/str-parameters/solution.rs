/// The first whitespace-separated word of `s`, borrowed from `s`; "" when there is none.
pub fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

/// The extension of the file name (the text after the last '/'), borrowed from `path`.
pub fn extension(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() {
        None
    } else {
        Some(ext)
    }
}

/// The non-empty parts, joined with `sep`.
pub fn join_nonempty<S: AsRef<str>>(parts: &[S], sep: &str) -> String {
    let kept: Vec<&str> = parts
        .iter()
        .map(|p| p.as_ref())
        .filter(|p| !p.is_empty())
        .collect();
    kept.join(sep)
}

/// How many of `words` equal `needle`, ignoring ASCII case.
pub fn count_word<I>(words: I, needle: &str) -> usize
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    words
        .into_iter()
        .filter(|w| w.as_ref().eq_ignore_ascii_case(needle))
        .count()
}
