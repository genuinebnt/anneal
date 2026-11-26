//! The `Cow` pattern: borrow when nothing changes, allocate only when something does.

use std::borrow::Cow;

fn is_special(c: char) -> bool {
    matches!(c, '<' | '>' | '&' | '"' | '\'')
}

/// Escapes text for HTML element content and quoted attribute values.
/// Input with nothing to escape comes back as the same slice.
pub fn escape_html(input: &str) -> Cow<'_, str> {
    let Some(first) = input.find(is_special) else {
        return Cow::Borrowed(input);
    };
    let mut out = String::with_capacity(input.len() + 8);
    out.push_str(&input[..first]); // already checked: copy in one go
    push_escaped(&input[first..], &mut out);
    Cow::Owned(out)
}

/// The alternative without `Cow`: the caller owns the buffer and can reuse it.
pub fn push_escaped(input: &str, out: &mut String) {
    for c in input.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}

/// Mutate only when needed: `to_mut` clones a borrowed value the moment it is called.
pub fn ensure_trailing_slash(path: &mut Cow<'_, str>) {
    if !path.ends_with('/') {
        path.to_mut().push('/');
    }
}

/// Wrong: clones even when there is nothing to lowercase.
pub fn lowercase_eager(name: &mut Cow<'_, str>) {
    name.to_mut().make_ascii_lowercase();
}

/// Right: check first, then call `to_mut`.
pub fn lowercase_lazy(name: &mut Cow<'_, str>) {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        name.to_mut().make_ascii_lowercase();
    }
}

/// Mostly string literals, sometimes a formatted message. No lifetime to carry around.
pub fn status_text(code: u16) -> Cow<'static, str> {
    match code {
        200 => Cow::Borrowed("ok"),
        404 => Cow::Borrowed("not found"),
        other => Cow::Owned(format!("status {other}")),
    }
}

/// A struct that borrows from its input when it can.
#[derive(Debug, Clone, PartialEq)]
pub struct Label<'a> {
    pub text: Cow<'a, str>,
}

impl<'a> Label<'a> {
    /// `&str` and `&String` stay borrowed; a `String` is moved in without a copy.
    pub fn new(text: impl Into<Cow<'a, str>>) -> Self {
        Label { text: text.into() }
    }

    /// Cuts the tie to the input. `std` has no method that does this for you.
    pub fn into_static(self) -> Label<'static> {
        Label {
            text: Cow::Owned(self.text.into_owned()),
        }
    }
}

/// The same pattern for slices: borrow input that is already sorted.
pub fn sorted(xs: &[u32]) -> Cow<'_, [u32]> {
    if xs.is_sorted() {
        Cow::Borrowed(xs)
    } else {
        let mut v = xs.to_vec();
        v.sort_unstable();
        Cow::Owned(v)
    }
}

/// You own the bytes and want a `String`. Reuses the buffer when it is valid UTF-8.
/// `String::from_utf8_lossy(&bytes).into_owned()` would copy it.
pub fn lossy_from_vec(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    }
}
