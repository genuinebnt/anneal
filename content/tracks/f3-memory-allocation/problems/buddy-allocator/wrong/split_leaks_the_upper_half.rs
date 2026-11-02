use std::collections::BTreeSet;

/// A buddy allocator over `2^max_order` units. Blocks are powers of two and aligned to their size, so a block's
/// buddy is found by flipping one bit of its offset.
pub struct Buddy {
    max_order: u32,
    free: Vec<BTreeSet<usize>>, // free[k]: offsets of free blocks of 2^k units
}

fn order_for(size: usize) -> u32 {
    size.max(1).next_power_of_two().trailing_zeros()
}

impl Buddy {
    pub fn new(max_order: u32) -> Self {
        let mut free = vec![BTreeSet::new(); max_order as usize + 1];
        free[max_order as usize].insert(0);
        Buddy { max_order, free }
    }

    /// Offset of a block of at least `size` units (rounded up to a power of two, 0 counts as 1), lowest address first.
    pub fn alloc(&mut self, size: usize) -> Option<usize> {
        if size > (1usize << self.max_order) {
            return None;
        }
        let want = order_for(size);
        let mut k = (want..=self.max_order).find(|&k| !self.free[k as usize].is_empty())?;
        let off = self.free[k as usize].pop_first().unwrap();
        while k > want {
            k -= 1;
            let _ = off + (1 << k);
        }
        Some(off)
    }

    /// Return a block obtained from `alloc(size)` (the same `size`). Merges with its buddy while the buddy is free.
    pub fn free(&mut self, offset: usize, size: usize) {
        let mut off = offset;
        let mut k = order_for(size);
        while k < self.max_order {
            let buddy = off ^ (1 << k);
            if !self.free[k as usize].remove(&buddy) {
                break;
            }
            off = off.min(buddy);
            k += 1;
        }
        self.free[k as usize].insert(off);
    }

    /// Total units currently free.
    pub fn free_units(&self) -> usize {
        self.free.iter().enumerate().map(|(k, s)| s.len() << k).sum()
    }

    /// The largest block `alloc` could hand out right now, in units (0 if none).
    pub fn largest_free(&self) -> usize {
        (0..=self.max_order).rev().find(|&k| !self.free[k as usize].is_empty()).map_or(0, |k| 1 << k)
    }
}
