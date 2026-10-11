//! A bitmap of which pages of a file are in use.

#[derive(Debug, PartialEq, Eq)]
pub enum FreeError {
    DoubleFree,
    OutOfRange,
}

pub struct PageBitmap {
    _map: (),
}

impl PageBitmap {
    /// A bitmap for `capacity` pages, all free.
    pub fn new(capacity: usize) -> PageBitmap {
        todo!("1a-c3: every page free")
    }

    /// The lowest free page, marked used.
    pub fn allocate(&mut self) -> Option<usize> {
        todo!("1a-c3: find the lowest free page")
    }

    /// Gives `page` back. `Err(OutOfRange)` for a page beyond the capacity, `Err(DoubleFree)` for one that is not in use.
    pub fn free(&mut self, page: usize) -> Result<(), FreeError> {
        todo!("1a-c3: give the page back, or say why not")
    }

    /// Whether `page` is in use; `false` past the capacity.
    pub fn is_allocated(&self, page: usize) -> bool {
        todo!("1a-c3: is this page in use")
    }

    /// The number of pages in use.
    pub fn used(&self) -> usize {
        todo!("1a-c3: how many pages are in use")
    }
}
