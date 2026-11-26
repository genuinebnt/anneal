#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetError {
    UnknownEscape(usize),
    TrailingEscape(usize),
    Reversed(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharSet {
    // your fields
}

impl CharSet {
    pub fn parse(spec: &str) -> Result<CharSet, SetError> {
        todo!()
    }

    /// Sorted pairs `(low, high)`, inclusive, which neither overlap nor touch.
    pub fn ranges(&self) -> &[(char, char)] {
        todo!()
    }

    pub fn contains(&self, c: char) -> bool {
        todo!()
    }
}
