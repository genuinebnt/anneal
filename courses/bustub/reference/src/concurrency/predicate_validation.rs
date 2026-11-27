//! Validating a serializable transaction's predicate reads against later writes.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pred {
    Eq(usize, i64),
    Range(usize, i64, i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    pub old: Option<Vec<i64>>,
    pub new: Option<Vec<i64>>,
}

fn matches(p: &Pred, row: &[i64]) -> bool {
    match p {
        Pred::Eq(c, v) => row.get(*c) == Some(v),
        Pred::Range(c, lo, hi) => row.get(*c).is_some_and(|x| lo <= x && x <= hi),
    }
}

/// True when no write could have changed what any predicate returned.
pub fn validate(preds: &[Pred], writes: &[Write]) -> bool {
    // @begin 4b-c4
    !writes.iter().any(|w| [&w.old, &w.new].into_iter().flatten().any(|row| preds.iter().any(|p| matches(p, row))))
    //~ todo!("4b-c4: a write conflicts when its old or its new row satisfies a predicate")
    // @end
}
