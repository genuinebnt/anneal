use std::borrow::Cow;

/// `text` lowercased, with its words joined by "-". When `text` is already in that form (no whitespace, no
/// char that lowercasing changes), the result borrows `text` instead of allocating.
pub fn slug(text: &str) -> Cow<'_, str> {
    if text
        .chars()
        .all(|c| !c.is_whitespace() && c.to_lowercase().eq([c]))
    {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .to_lowercase(),
    )
}

/// The non-blank lines of `text`, trimmed, in order.
pub fn clean_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

/// The file name (the part after the last '/') of the path with the longest file name; the first on a tie.
pub fn longest_file_name(paths: &[String]) -> Option<&str> {
    let mut best: Option<&str> = None;
    for p in paths {
        let name = p.rsplit('/').next().unwrap();
        if best.map_or(true, |b| name.len() > b.len()) {
            best = Some(name);
        }
    }
    best
}
