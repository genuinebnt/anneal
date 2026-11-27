use std::collections::BTreeMap;

/// The most brackets that may be open at once.
pub const MAX_OPEN: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitError {
    BadChar(usize),
    Unexpected(usize),
    UnexpectedEnd,
    TooDeep(usize),
    TooBig,
}

/// Each unit name with its power; names with power 0 are left out.
pub fn parse(text: &str) -> Result<BTreeMap<String, i32>, UnitError> {
    todo!()
}
