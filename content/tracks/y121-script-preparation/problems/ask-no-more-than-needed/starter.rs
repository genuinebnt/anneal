/// A rule over yes-or-no tests. Each test is a slow question to a device.
#[derive(Debug, Clone)]
pub enum Rule {
    Test(u32),
    Not(Box<Rule>),
    /// True if every part is true. With no parts, true.
    All(Vec<Rule>),
    /// True if some part is true. With no parts, false.
    Any(Vec<Rule>),
    /// The second rule if the first is true, else the third.
    Choose(Box<Rule>, Box<Rule>, Box<Rule>),
}

/// One instruction of a machine with a stack of booleans. There is no way backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    /// Asks the device and pushes the answer.
    Ask(u32),
    Push(bool),
    /// Replaces the top by its opposite.
    Not,
    Pop,
    /// Skips the next `n` instructions.
    Skip(usize),
    /// Skips the next `n` instructions if the top is false. The top stays.
    SkipIfFalse(usize),
}

/// Turns a rule into instructions that leave its value as the only thing on the stack.
pub fn compile(rule: &Rule) -> Vec<Instr> {
    todo!()
}
