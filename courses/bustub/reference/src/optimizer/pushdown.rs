//! Splitting a filter so that parts can be applied below a join.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Eq,
    Gt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pred {
    Cmp(usize, CmpOp, i64),
    ColCmp(usize, CmpOp, usize),
    And(Vec<Pred>),
    Or(Vec<Pred>),
    Not(Box<Pred>),
}

fn apply(op: CmpOp, a: i64, b: i64) -> bool {
    match op {
        CmpOp::Lt => a < b,
        CmpOp::Eq => a == b,
        CmpOp::Gt => a > b,
    }
}

/// Does `p` hold on `row`?
pub fn holds(p: &Pred, row: &[i64]) -> bool {
    match p {
        Pred::Cmp(c, op, v) => apply(*op, row[*c], *v),
        Pred::ColCmp(a, op, b) => apply(*op, row[*a], row[*b]),
        Pred::And(ps) => ps.iter().all(|q| holds(q, row)),
        Pred::Or(ps) => ps.iter().any(|q| holds(q, row)),
        Pred::Not(q) => !holds(q, row),
    }
}

/// The columns a predicate mentions.
pub fn columns(p: &Pred) -> Vec<usize> {
    match p {
        Pred::Cmp(c, _, _) => vec![*c],
        Pred::ColCmp(a, _, b) => vec![*a, *b],
        Pred::And(ps) | Pred::Or(ps) => ps.iter().flat_map(columns).collect(),
        Pred::Not(q) => columns(q),
    }
}

/// The conjuncts of `p`: nested ANDs are flattened, anything else is one conjunct.
pub fn conjuncts(p: &Pred) -> Vec<Pred> {
    // @begin 3h-c2
    match p {
        Pred::And(ps) => ps.iter().flat_map(conjuncts).collect(),
        other => vec![other.clone()],
    }
    //~ todo!("3h-c2: flatten nested ANDs; anything else is one conjunct")
    // @end
}

/// `(left only, right only, both)`.
pub fn split_for_join(p: &Pred, left_cols: usize) -> (Vec<Pred>, Vec<Pred>, Vec<Pred>) {
    // @begin 3h-c2
    let (mut l, mut r, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for c in conjuncts(p) {
        let cols = columns(&c);
        if cols.iter().all(|&i| i < left_cols) {
            l.push(c);
        } else if cols.iter().all(|&i| i >= left_cols) {
            r.push(c);
        } else {
            b.push(c);
        }
    }
    (l, r, b)
    //~ todo!("3h-c2: sort the conjuncts by which side's columns they mention")
    // @end
}
