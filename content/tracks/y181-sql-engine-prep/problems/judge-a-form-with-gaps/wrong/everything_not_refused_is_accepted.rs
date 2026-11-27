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

fn known(yes: bool) -> Verdict {
    if yes {
        Verdict::Yes
    } else {
        Verdict::No
    }
}

/// `All` and `Any` differ only in which verdict settles the answer by itself.
fn combine(
    parts: &[Rule],
    answers: &[Option<i64>],
    settles: Verdict,
    otherwise: Verdict,
) -> Verdict {
    let mut result = otherwise;
    for part in parts {
        match judge(part, answers) {
            v if v == settles => return settles,
            Verdict::Unknown => result = Verdict::Unknown,
            _ => {}
        }
    }
    result
}

/// What `rule` says about one form.
pub fn judge(rule: &Rule, answers: &[Option<i64>]) -> Verdict {
    let answer = |q: usize| answers.get(q).copied().flatten();
    match rule {
        Rule::AtLeast(q, n) => answer(*q).map_or(Verdict::Unknown, |a| known(a >= *n)),
        Rule::Same(a, b) => match (answer(*a), answer(*b)) {
            (Some(a), Some(b)) => known(a == b),
            _ => Verdict::Unknown,
        },
        Rule::Missing(q) => known(answer(*q).is_none()),
        Rule::Not(of) => match judge(of, answers) {
            Verdict::Yes => Verdict::No,
            Verdict::No => Verdict::Yes,
            Verdict::Unknown => Verdict::Unknown,
        },
        Rule::All(parts) => combine(parts, answers, Verdict::No, Verdict::Yes),
        Rule::Any(parts) => combine(parts, answers, Verdict::Yes, Verdict::No),
    }
}

/// The numbers of the forms to accept, and of those to send back for missing answers.
pub fn sort_forms(rule: &Rule, forms: &[Vec<Option<i64>>]) -> (Vec<usize>, Vec<usize>) {
    let with = |wanted| {
        (0..forms.len())
            .filter(|&i| judge(rule, &forms[i]) == wanted)
            .collect()
    };
    let accepted = (0..forms.len())
        .filter(|&i| judge(rule, &forms[i]) != Verdict::No)
        .collect();
    (accepted, with(Verdict::Unknown))
}
