//! Pin counts: how many users hold each page.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub struct NotPinned;

pub struct PinTable {
    _pins: (),
}

impl PinTable {
    /// A table in which no page is pinned.
    pub fn new() -> PinTable {
        todo!("1f-c1: no page is pinned")
    }

    /// Adds a pin to `page` and returns its new count.
    pub fn pin(&mut self, page: u32) -> usize {
        todo!("1f-c1: one more pin; the new count")
    }

    /// Removes a pin from `page` and returns the count left.
    ///
    /// # Errors
    ///
    /// Returns [`NotPinned`] if the page has no pin.
    pub fn unpin(&mut self, page: u32) -> Result<usize, NotPinned> {
        todo!("1f-c1: one pin fewer, or an error when there is none")
    }

    /// The number of pins on `page`; 0 if it has none.
    pub fn count(&self, page: u32) -> usize {
        todo!("1f-c1: how many pins")
    }

    pub fn is_pinned(&self, page: u32) -> bool {
        self.count(page) > 0
    }

    /// The pages with at least one pin, in increasing order.
    pub fn pinned_pages(&self) -> Vec<u32> {
        todo!("1f-c1: the pinned pages, sorted")
    }
}

impl Default for PinTable {
    fn default() -> Self {
        PinTable::new()
    }
}
