#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// `None` when nothing at all was written.
    pub value: Option<String>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Unclosed,
    AfterQuote,
    StrayQuote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadLine {
    pub problem: Problem,
    pub start: usize,
    pub end: usize,
}

/// The fields of one line, or its first mistake.
pub fn fields(line: &str) -> Result<Vec<Field>, BadLine> {
    let _ = line;
    todo!()
}
