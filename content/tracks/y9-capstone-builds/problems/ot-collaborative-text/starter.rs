/// One edit by one site (a user), at **character** positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Insert `ch` so that it ends up at index `pos` (`0..=len`). `site` identifies the author and breaks ties.
    Insert { pos: usize, ch: char, site: u32 },
    /// Delete the character at index `pos` (`0..len`).
    Delete { pos: usize },
    /// Does nothing: what a delete becomes when the same character was already deleted.
    Nop,
}

/// Applies one op to a string. (Provided.)
pub fn apply(s: &str, op: &Op) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    match op {
        Op::Insert { pos, ch, .. } => chars.insert(*pos, *ch),
        Op::Delete { pos } => {
            chars.remove(*pos);
        }
        Op::Nop => {}
    }
    chars.into_iter().collect()
}

/// Applies a sequence of ops, in order. (Provided.)
pub fn apply_all(s: &str, ops: &[Op]) -> String {
    ops.iter().fold(s.to_string(), |acc, op| apply(&acc, op))
}

/// Given two ops `a` and `b` made **concurrently** against the same text, returns `(a', b')` such that
/// `apply(apply(s, a), b') == apply(apply(s, b), a')`: `b'` is `b` as it must be applied *after* `a`, and `a'` is `a` as
/// it must be applied *after* `b`.
///
/// Two inserts at the same index are ordered by site: the **lower** site's character comes first (to the left).
pub fn transform(a: &Op, b: &Op) -> (Op, Op) {
    todo!()
}

/// The same for two whole sequences: `a` and `b` are lists of ops that were each made one after another against the same
/// starting text. Returns `(a', b')` with `apply_all(apply_all(s, a), b') == apply_all(apply_all(s, b), a')`.
pub fn transform_lists(a: &[Op], b: &[Op]) -> (Vec<Op>, Vec<Op>) {
    todo!()
}
