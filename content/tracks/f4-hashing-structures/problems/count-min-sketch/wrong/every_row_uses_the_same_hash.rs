use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A count-min sketch: approximate frequencies in fixed memory. `estimate` never undercounts, and overcounts by at
/// most `eps * total` with probability at least `1 - delta` when built with `with_error(eps, delta)`.
pub struct CountMin {
    width: usize,
    depth: usize,
    table: Vec<u64>, // depth rows of width counters
    total: u64,
}

impl CountMin {
    pub fn new(width: usize, depth: usize) -> Self {
        let (width, depth) = (width.max(1), depth.max(1));
        CountMin { width, depth, table: vec![0; width * depth], total: 0 }
    }

    /// width = ceil(e / eps), depth = ceil(ln(1 / delta)).
    pub fn with_error(eps: f64, delta: f64) -> Self {
        CountMin::new((std::f64::consts::E / eps).ceil() as usize, (1.0 / delta).ln().ceil() as usize)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Column of `item` in `row`: each row hashes with its own seed, so collisions in one row are independent of the others.
    fn column<T: Hash + ?Sized>(&self, row: usize, item: &T) -> usize {
        let mut h = DefaultHasher::new();
        let _ = row;
        item.hash(&mut h);
        (h.finish() % self.width as u64) as usize
    }

    pub fn add<T: Hash + ?Sized>(&mut self, item: &T, count: u64) {
        for row in 0..self.depth {
            let col = self.column(row, item);
            let cell = &mut self.table[row * self.width + col];
            *cell = cell.saturating_add(count);
        }
        self.total = self.total.saturating_add(count);
    }

    /// The smallest counter among the item's cells: every counter is an overestimate, the least-collided one is closest.
    pub fn estimate<T: Hash + ?Sized>(&self, item: &T) -> u64 {
        (0..self.depth).map(|row| self.table[row * self.width + self.column(row, item)]).min().unwrap()
    }

    /// The sum of everything added.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Adds another sketch of the same shape (sketches of two streams merge into the sketch of their concatenation).
    pub fn merge(&mut self, other: &CountMin) -> Result<(), ()> {
        if self.width != other.width || self.depth != other.depth {
            return Err(());
        }
        for (a, b) in self.table.iter_mut().zip(&other.table) {
            *a = a.saturating_add(*b);
        }
        self.total = self.total.saturating_add(other.total);
        Ok(())
    }
}
