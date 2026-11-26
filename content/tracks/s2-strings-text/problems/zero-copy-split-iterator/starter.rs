pub struct SplitOn<'a> {
    /// What hasn't been yielded yet; `None` once both ends have met.
    rest: Option<&'a str>,
    delim: char,
}

pub fn split_on(s: &str, delim: char) -> SplitOn<'_> {
    SplitOn {
        rest: Some(s),
        delim,
    }
}

impl<'a> Iterator for SplitOn<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        todo!()
    }
}

impl DoubleEndedIterator for SplitOn<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
