//! A fixed-size page of variable-length records.

pub struct SlottedPage {
    _page: (),
}

const SLOT_COST: usize = 4;

impl SlottedPage {
    /// An empty page of `size` bytes.
    pub fn new(size: usize) -> SlottedPage {
        todo!("2a-c3: an empty page of `size` bytes")
    }

    /// The bytes not used by records and their slot entries.
    pub fn free_space(&self) -> usize {
        todo!("2a-c3: the bytes not used by records and their slot entries")
    }

    /// Stores `record` in the lowest free slot and returns the slot; `None` if the record and its slot entry do not fit in the free space.
    pub fn insert(&mut self, record: &[u8]) -> Option<usize> {
        todo!("2a-c3: the lowest free slot, if the record fits in the total free space")
    }

    /// The record in `slot`; `None` for a free or unknown slot.
    pub fn get(&self, slot: usize) -> Option<&[u8]> {
        todo!("2a-c3: the record in the slot")
    }

    /// Frees `slot`; returns whether it held a record.
    pub fn delete(&mut self, slot: usize) -> bool {
        todo!("2a-c3: free the slot")
    }

    /// The number of records.
    pub fn len(&self) -> usize {
        todo!("2a-c3: how many records")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
