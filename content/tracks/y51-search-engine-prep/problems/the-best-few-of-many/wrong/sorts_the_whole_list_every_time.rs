use std::cmp::Ordering;

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
    /// greatest first
    kept: Vec<T>,
}

impl<T: Ord> Best<T> {
    pub fn new(k: usize) -> Best<T> {
        Best {
            k,
            kept: Vec::new(),
        }
    }

    pub fn offer(&mut self, item: T) -> bool {
        if self.kept.len() >= self.k {
            if !self.kept.last().is_some_and(|worst| item > *worst) {
                return false;
            }
            self.kept.pop();
        }
        self.kept.push(item);
        // sort again after every offer
        self.kept.sort_by(|a, b| b.cmp(a));
        true
    }

    pub fn cutoff(&self) -> Option<&T> {
        if self.kept.len() < self.k {
            return None;
        }
        self.kept.last()
    }

    pub fn into_sorted(self) -> Vec<T> {
        self.kept
    }
}
