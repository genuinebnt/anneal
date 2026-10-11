//! Iterating the live keys of a chain of leaves in which deleted keys are left behind as tombstones.

/// One slot of a leaf: a key that is there, or a deleted key left in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Live(i64),
    Tombstone,
}

/// Walks the leaves from left to right and yields every live key, in order, skipping tombstones.
pub struct LiveKeys<'a> {
    leaves: &'a [Vec<Slot>],
    leaf: usize,
    slot: usize,
}

impl<'a> LiveKeys<'a> {
    pub fn new(leaves: &'a [Vec<Slot>]) -> LiveKeys<'a> {
        LiveKeys { leaves, leaf: 0, slot: 0 }
    }
}

impl Iterator for LiveKeys<'_> {
    type Item = i64;

    fn next(&mut self) -> Option<i64> {
        // @begin 2d-c1
        loop {
            let leaf = self.leaves.get(self.leaf)?;
            match leaf.get(self.slot) {
                None => {
                    self.leaf += 1;
                    self.slot = 0;
                }
                Some(Slot::Live(k)) => {
                    self.slot += 1;
                    return Some(*k);
                }
                Some(Slot::Tombstone) => self.slot += 1,
            }
        }
        //~ // TODO(2d-c1): this looks right and has a bug; find it and fix it
        //~ let leaf = self.leaves.get(self.leaf)?;
        //~ while self.slot < leaf.len() {
        //~     let s = leaf[self.slot];
        //~     self.slot += 1;
        //~     if let Slot::Live(k) = s {
        //~         return Some(k);
        //~     }
        //~ }
        //~ self.leaf += 1;
        //~ self.slot = 0;
        //~ match self.leaves.get(self.leaf)?.first() {
        //~     Some(Slot::Live(k)) => {
        //~         self.slot = 1;
        //~         Some(*k)
        //~     }
        //~     _ => None,
        //~ }
        // @end
    }
}
