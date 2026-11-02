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

pub fn transform(a: &Op, b: &Op) -> (Op, Op) {
    use Op::*;
    match (a, b) {
        (Nop, _) | (_, Nop) => (a.clone(), b.clone()),
        (Insert { pos: pa, site: sa, .. }, Insert { pos: pb, site: sb, .. }) => {
            if pa < pb || (pa == pb && sa > sb) {
                // a lands to the left of b: b shifts right
                (a.clone(), shift_insert(b, 1))
            } else {
                (shift_insert(a, 1), b.clone())
            }
        }
        (Insert { pos: pa, .. }, Delete { pos: pb }) => {
            if pa <= pb {
                (a.clone(), Delete { pos: pb + 1 }) // the deleted character moved right
            } else {
                (shift_back(a), b.clone()) // the insert point moved left
            }
        }
        (Delete { .. }, Insert { .. }) => {
            let (b2, a2) = transform(b, a);
            (a2, b2)
        }
        (Delete { pos: pa }, Delete { pos: pb }) => {
            if pa < pb {
                (a.clone(), Delete { pos: pb - 1 })
            } else if pa > pb {
                (Delete { pos: pa - 1 }, b.clone())
            } else {
                (Nop, Nop) // both removed the same character
            }
        }
    }
}

fn shift_insert(op: &Op, by: usize) -> Op {
    match op {
        Op::Insert { pos, ch, site } => Op::Insert { pos: pos + by, ch: *ch, site: *site },
        other => other.clone(),
    }
}

fn shift_back(op: &Op) -> Op {
    match op {
        Op::Insert { pos, ch, site } => Op::Insert { pos: pos - 1, ch: *ch, site: *site },
        other => other.clone(),
    }
}

pub fn transform_lists(a: &[Op], b: &[Op]) -> (Vec<Op>, Vec<Op>) {
    let mut b_cur = b.to_vec();
    let mut a_out = Vec::with_capacity(a.len());
    for a_op in a {
        let mut a_cur = a_op.clone();
        for b_op in b_cur.iter_mut() {
            let (a2, b2) = transform(&a_cur, b_op);
            a_cur = a2;
            *b_op = b2;
        }
        a_out.push(a_cur);
    }
    (a_out, b_cur)
}
