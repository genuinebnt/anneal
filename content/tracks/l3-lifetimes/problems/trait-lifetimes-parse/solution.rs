use std::io::BufRead;

/// A type that can be parsed out of text living for `'a`. Types that borrow from the text implement it for
/// that `'a`; types that own their data implement it for every `'a`.
pub trait Parse<'a>: Sized {
    fn parse(s: &'a str) -> Option<Self>;
}

impl<'a> Parse<'a> for u32 {
    fn parse(s: &'a str) -> Option<Self> {
        s.trim().parse().ok()
    }
}

impl<'a> Parse<'a> for String {
    fn parse(s: &'a str) -> Option<Self> {
        Some(s.trim().to_string())
    }
}

#[derive(Debug, PartialEq)]
pub struct Pair<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

impl<'a> Parse<'a> for &'a str {
    fn parse(s: &'a str) -> Option<Self> {
        Some(s.trim())
    }
}

impl<'a> Parse<'a> for Pair<'a> {
    fn parse(s: &'a str) -> Option<Self> {
        let (key, value) = s.split_once('=')?;
        Some(Pair {
            key: key.trim(),
            value: value.trim(),
        })
    }
}

/// Parses every comma-separated field of `line`; None if any field fails. Results may borrow from `line`.
pub fn fields<'a, T: Parse<'a>>(line: &'a str) -> Option<Vec<T>> {
    line.split(',').map(T::parse).collect()
}

/// Reads one line from `input` and parses it. The line is gone by the time the caller gets the value, so
/// only types that own their data can come back.
pub fn read_owned<T>(mut input: impl BufRead) -> Option<T>
where
    T: for<'a> Parse<'a>,
{
    let mut line = String::new();
    input.read_line(&mut line).ok()?;
    T::parse(&line)
}
