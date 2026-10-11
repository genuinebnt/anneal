//! What a LIKE pattern says about the *range* of strings it can match.

#[derive(Debug, PartialEq, Eq)]
pub enum LikeRange {
    /// The pattern starts with a wildcard: no usable prefix.
    Everything,
    /// No wildcard at all: only this string matches.
    Exact(String),
    /// Every match starts with `prefix`; all of them are `< upper` (if there is one). `recheck`: the matches of the range still have to
    /// be tested against the pattern.
    Range { prefix: String, upper: Option<String>, recheck: bool },
}

/// The smallest string greater than every string that starts with `prefix`, if there is one.
pub fn successor(prefix: &str) -> Option<String> {
    // @begin 3i-c4
    let mut chars: Vec<char> = prefix.chars().collect();
    while let Some(last) = chars.pop() {
        let mut next = last as u32 + 1;
        if (0xD800..=0xDFFF).contains(&next) {
            next = 0xE000;
        }
        if let Some(c) = char::from_u32(next) {
            chars.push(c);
            return Some(chars.into_iter().collect());
        }
        // the largest character: drop it and increase the one before
    }
    None
    //~ todo!("3i-c4: increase the last character; if it cannot be increased, drop it and increase the previous one; None when nothing is left")
    // @end
}

/// What `pattern` says about the strings it matches: the literal prefix before the first wildcard (escapes resolved), then `Everything` for no
/// prefix, `Exact` for no wildcard at all, or a `Range` of strings that start with the prefix.
pub fn prefix_range(pattern: &str) -> LikeRange {
    // @begin 3i-c4
    let mut prefix = String::new();
    let mut chars = pattern.chars().peekable();
    let mut wildcard_at: Option<(char, String)> = None;
    while let Some(c) = chars.next() {
        match c {
            '%' | '_' => {
                wildcard_at = Some((c, chars.collect()));
                break;
            }
            '\\' => prefix.push(chars.next().unwrap_or('\\')),
            c => prefix.push(c),
        }
    }
    let Some((wildcard, rest)) = wildcard_at else {
        return LikeRange::Exact(prefix);
    };
    if prefix.is_empty() {
        return LikeRange::Everything;
    }
    // only "prefix%" (and "prefix%%...") is exactly the range
    let recheck = !(wildcard == '%' && rest.chars().all(|c| c == '%'));
    let upper = successor(&prefix);
    LikeRange::Range { prefix, upper, recheck }
    //~ todo!("3i-c4: the literal prefix (escapes resolved), then Everything / Exact / Range")
    // @end
}
