//! Pin counts: how many users hold each page.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub struct NotPinned;

pub struct PinTable {
    // @begin 1f-c1
    counts: BTreeMap<u32, usize>,
    //~ _pins: (),
    // @end
}

impl PinTable {
    /// A table in which no page is pinned.
    pub fn new() -> PinTable {
        // @begin 1f-c1
        PinTable { counts: BTreeMap::new() }
        //~ todo!("1f-c1: no page is pinned")
        // @end
    }

    /// Adds a pin to `page` and returns its new count.
    pub fn pin(&mut self, page: u32) -> usize {
        // @begin 1f-c1
        let c = self.counts.entry(page).or_insert(0);
        *c += 1;
        *c
        //~ todo!("1f-c1: one more pin; the new count")
        // @end
    }

    /// Removes a pin from `page` and returns the count left.
    ///
    /// # Errors
    ///
    /// Returns [`NotPinned`] if the page has no pin.
    pub fn unpin(&mut self, page: u32) -> Result<usize, NotPinned> {
        // @begin 1f-c1
        match self.counts.get_mut(&page) {
            None => Err(NotPinned),
            Some(c) if *c == 1 => {
                self.counts.remove(&page);
                Ok(0)
            }
            Some(c) => {
                *c -= 1;
                Ok(*c)
            }
        }
        //~ todo!("1f-c1: one pin fewer, or an error when there is none")
        // @end
    }

    /// The number of pins on `page`; 0 if it has none.
    pub fn count(&self, page: u32) -> usize {
        // @begin 1f-c1
        self.counts.get(&page).copied().unwrap_or(0)
        //~ todo!("1f-c1: how many pins")
        // @end
    }

    pub fn is_pinned(&self, page: u32) -> bool {
        self.count(page) > 0
    }

    /// The pages with at least one pin, in increasing order.
    pub fn pinned_pages(&self) -> Vec<u32> {
        // @begin 1f-c1
        self.counts.keys().copied().collect()
        //~ todo!("1f-c1: the pinned pages, sorted")
        // @end
    }
}

impl Default for PinTable {
    fn default() -> Self {
        PinTable::new()
    }
}
