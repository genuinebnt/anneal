/// A buddy allocator over `2^max_order` units. Blocks are powers of two and aligned to their size, so a block's
/// buddy is found by flipping one bit of its offset.
pub struct Buddy {
    // TODO: a set of free offsets per order
}

impl Buddy {
    pub fn new(max_order: u32) -> Self {
        todo!()
    }

    /// Offset of a block of at least `size` units (rounded up to a power of two, 0 counts as 1), lowest address first.
    /// `None` if nothing big enough is free.
    pub fn alloc(&mut self, size: usize) -> Option<usize> {
        todo!()
    }

    /// Return a block obtained from `alloc(size)` (the same `size`). Merges with its buddy while the buddy is free.
    pub fn free(&mut self, offset: usize, size: usize) {
        todo!()
    }

    /// Total units currently free.
    pub fn free_units(&self) -> usize {
        todo!()
    }

    /// The largest block `alloc` could hand out right now, in units (0 if none).
    pub fn largest_free(&self) -> usize {
        todo!()
    }
}
