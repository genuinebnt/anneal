/// One score of one player. Never holds NaN.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    id: u64,
    score: f64,
}

impl Entry {
    /// `None` for a NaN score.
    pub fn new(id: u64, score: f64) -> Option<Entry> {
        todo!()
    }

    /// The id and the score.
    pub fn parts(&self) -> (u64, f64) {
        (self.id, self.score)
    }
}

// Greater means better. Replace the bodies.
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        todo!()
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        todo!()
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        todo!()
    }
}

impl Eq for Entry {}

pub struct Best<T: Ord> {
    // your fields; `items` only lets the starter compile
    items: Vec<T>,
}

impl<T: Ord> Best<T> {
    pub fn new(k: usize) -> Best<T> {
        todo!()
    }

    /// Considers one item; true if it is kept.
    pub fn offer(&mut self, item: T) -> bool {
        todo!()
    }

    /// The worst item kept, once the list is full.
    pub fn cutoff(&self) -> Option<&T> {
        todo!()
    }

    /// The kept items, greatest first.
    pub fn into_sorted(self) -> Vec<T> {
        todo!()
    }
}
