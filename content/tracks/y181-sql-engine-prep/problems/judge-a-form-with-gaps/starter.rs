#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// Answer `q` is `n` or more.
    AtLeast(usize, i64),
    /// Two answers are equal.
    Same(usize, usize),
    /// Answer `q` was not given.
    Missing(usize),
    Not(Box<Rule>),
    All(Vec<Rule>),
    Any(Vec<Rule>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Yes,
    No,
    Unknown,
}

/// What `rule` says about one form.
pub fn judge(rule: &Rule, answers: &[Option<i64>]) -> Verdict {
    let _ = (rule, answers);
    todo!()
}

/// The numbers of the forms to accept, and of those to send back for missing answers.
pub fn sort_forms(rule: &Rule, forms: &[Vec<Option<i64>>]) -> (Vec<usize>, Vec<usize>) {
    let _ = (rule, forms);
    todo!()
}
