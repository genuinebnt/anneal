#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedChar(usize),
    UnexpectedToken(usize),
    UnexpectedEnd(usize),
    ChainedComparison(usize),
    BadAssignTarget(usize),
    TooDeep(usize),
}

pub fn parse(source: &str) -> Result<String, ParseError> {
    todo!()
}
