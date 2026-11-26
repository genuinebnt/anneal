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
        let rest = self.rest?;
        match rest.find(self.delim) {
            Some(i) => {
                self.rest = Some(&rest[i + self.delim.len_utf8()..]);
                Some(&rest[..i])
            }
            None => {
                self.rest = None;
                Some(rest)
            }
        }
    }
}

impl DoubleEndedIterator for SplitOn<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let rest = self.rest?;
        match rest.rfind(self.delim) {
            Some(i) => {
                self.rest = Some(&rest[..i]);
                Some(&rest[i + self.delim.len_utf8()..])
            }
            None => {
                self.rest = None;
                Some(rest)
            }
        }
    }
}
