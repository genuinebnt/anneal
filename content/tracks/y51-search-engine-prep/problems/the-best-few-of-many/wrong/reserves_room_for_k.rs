use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

/// One score of one player. Never holds NaN.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    id: u64,
    score: f64,
}

impl Entry {
    /// `None` for a NaN score.
    pub fn new(id: u64, score: f64) -> Option<Entry> {
        (!score.is_nan()).then_some(Entry { id, score })
    }

    /// The id and the score.
    pub fn parts(&self) -> (u64, f64) {
        (self.id, self.score)
    }
}

/// Greater means better.
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.id.cmp(&self.id))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Equality has to agree with `Ord`, so it is not derived from the float.
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Entry {}

pub struct Best<T: Ord> {
    k: usize,
    /// `Reverse` puts the worst kept item on top
    kept: BinaryHeap<Reverse<T>>,
}

impl<T: Ord> Best<T> {
    pub fn new(k: usize) -> Best<T> {
        Best {
            k,
            kept: BinaryHeap::with_capacity(k.saturating_add(1)),
        }
    }

    /// Considers one item; true if it is kept.
    pub fn offer(&mut self, item: T) -> bool {
        if self.kept.len() < self.k {
            self.kept.push(Reverse(item));
            return true;
        }
        // here the heap is empty only when k is 0
        match self.kept.peek_mut() {
            Some(mut worst) if item > worst.0 => {
                *worst = Reverse(item);
                true
            }
            _ => false,
        }
    }

    /// The worst item kept, once the list is full.
    pub fn cutoff(&self) -> Option<&T> {
        if self.kept.len() < self.k {
            return None;
        }
        self.kept.peek().map(|Reverse(item)| item)
    }

    /// The kept items, greatest first.
    pub fn into_sorted(self) -> Vec<T> {
        // ascending order of `Reverse` is greatest first
        self.kept
            .into_sorted_vec()
            .into_iter()
            .map(|Reverse(item)| item)
            .collect()
    }
}
