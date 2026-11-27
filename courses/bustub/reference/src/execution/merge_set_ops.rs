//! Set operations on sorted streams, by merging.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Union,
    Intersect,
    Except,
}

/// `left op [ALL] right` for two ascending iterators; the result is ascending and lazy.
pub fn merge_set_op<'a>(
    left: impl Iterator<Item = i64> + 'a,
    right: impl Iterator<Item = i64> + 'a,
    op: Op,
    all: bool,
) -> Box<dyn Iterator<Item = i64> + 'a> {
    // @begin 3j-c4
    struct Merge<L: Iterator<Item = i64>, R: Iterator<Item = i64>> {
        l: std::iter::Peekable<L>,
        r: std::iter::Peekable<R>,
        op: Op,
        all: bool,
        /// A value still to be handed out, and how many more copies.
        pending: (i64, usize),
    }
    impl<L: Iterator<Item = i64>, R: Iterator<Item = i64>> Iterator for Merge<L, R> {
        type Item = i64;
        fn next(&mut self) -> Option<i64> {
            loop {
                if self.pending.1 > 0 {
                    self.pending.1 -= 1;
                    return Some(self.pending.0);
                }
                let key = match (self.l.peek().copied(), self.r.peek().copied()) {
                    (None, None) => return None,
                    (Some(a), None) => a,
                    (None, Some(b)) => b,
                    (Some(a), Some(b)) => a.min(b),
                };
                let (mut na, mut nb) = (0usize, 0usize);
                while self.l.peek() == Some(&key) {
                    self.l.next();
                    na += 1;
                }
                while self.r.peek() == Some(&key) {
                    self.r.next();
                    nb += 1;
                }
                let n = match (self.op, self.all) {
                    (Op::Union, true) => na + nb,
                    (Op::Union, false) => 1,
                    (Op::Intersect, true) => na.min(nb),
                    (Op::Intersect, false) => usize::from(na > 0 && nb > 0),
                    (Op::Except, true) => na.saturating_sub(nb),
                    (Op::Except, false) => usize::from(na > 0 && nb == 0),
                };
                self.pending = (key, n);
            }
        }
    }
    Box::new(Merge { l: left.peekable(), r: right.peekable(), op, all, pending: (0, 0) })
    //~ let _ = (left, right, op, all);
    //~ todo!("3j-c4: peek both heads, take the smaller value, count its run on each side, emit as many copies as the operation says")
    // @end
}
