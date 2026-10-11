//! Folding the constant conditions of a CASE.

/// A condition: a literal, or something that depends on the row (named by a number).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    True,
    False,
    Null,
    Unknown(u32),
}

/// `CASE WHEN c1 THEN r1 ... [ELSE e] END`; results are opaque ids, `otherwise: None` is no ELSE (NULL).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub branches: Vec<(Cond, u32)>,
    pub otherwise: Option<u32>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Folded {
    /// Decided: the id of the result, `None` for NULL.
    Result(Option<u32>),
    /// Still depends on the row.
    Case(Case),
}

/// Simplifies a `CASE`: FALSE and NULL branches are dropped, a TRUE branch ends the list and becomes the ELSE, and a `CASE` with no branch left is
/// its ELSE (NULL without one).
pub fn fold_case(case: &Case) -> Folded {
    todo!("3i-c3: drop FALSE and NULL branches; a TRUE branch ends the list and becomes the ELSE; nothing left means the ELSE")
}
