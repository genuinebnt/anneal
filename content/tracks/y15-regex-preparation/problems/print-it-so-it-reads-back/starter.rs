#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Formula {
    Var(char),
    Not(Box<Formula>),
    And(Vec<Formula>),
    Or(Vec<Formula>),
}

/// Text that reads back as exactly this tree, with no unnecessary brackets.
pub fn to_text(formula: &Formula) -> String {
    todo!()
}

/// A made-up formula of height at most `max_height`, the same for the same arguments.
pub fn sample(seed: u64, max_height: usize) -> Formula {
    todo!()
}
