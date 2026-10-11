//! Sizes and identifiers shared by the whole database.

/// Size of a data page in bytes.
pub const BUSTUB_PAGE_SIZE: usize = 8192;

/// The disk manager's db file starts with room for this many pages.
pub const DEFAULT_DB_IO_SIZE: usize = 16;

/// One page of data: what travels between memory and disk.
pub type PageData = [u8; BUSTUB_PAGE_SIZE];

/// A page's id. [`PageId::INVALID`] is for on-disk formats that must store "no page"; in memory prefer `Option<PageId>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageId(pub i32);

impl PageId {
    pub const INVALID: PageId = PageId(-1);

    pub fn is_valid(self) -> bool {
        self.0 >= 0
    }
}

/// The index of a frame: a slot in the buffer pool's memory. "No frame" is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameId(pub usize);
