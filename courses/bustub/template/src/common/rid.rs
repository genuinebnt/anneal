//! A record id: names a tuple by the page it lives in and its slot on that page.

use super::config::PageId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rid {
    page_id: PageId,
    slot_num: u32,
}

impl Default for Rid {
    /// The invalid rid, naming no page.
    fn default() -> Rid {
        Rid { page_id: PageId::INVALID, slot_num: 0 }
    }
}

impl Rid {
    pub fn new(page_id: PageId, slot_num: u32) -> Rid {
        Rid { page_id, slot_num }
    }

    pub fn page_id(&self) -> PageId {
        self.page_id
    }

    pub fn slot_num(&self) -> u32 {
        self.slot_num
    }

    /// The whole rid as one `i64`: the page id in the high 32 bits, the slot in the low 32.
    pub fn get(&self) -> i64 {
        todo!("2a-01: page id in the upper half, slot number in the lower half; mind the sign of the page id")
    }

    /// The inverse of [`Rid::get`].
    pub fn from_i64(rid: i64) -> Rid {
        todo!("2a-01: split the i64 back into page id and slot number")
    }
}
