//! Reading and writing integers inside a page. Bytes are read at a stated offset in a stated byte order: no alignment requirements, no padding
//! surprises, the same result on every machine.

use crate::common::config::PageId;

/// The `u32` stored little-endian at byte `offset` of `page`.
pub fn read_u32(page: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(page[offset..offset + 4].try_into().expect("a 4-byte slice"))
}

/// Stores `value` little-endian at byte `offset` of `page`.
pub fn write_u32(page: &mut [u8], offset: usize, value: u32) {
    page[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn read_u64(page: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(page[offset..offset + 8].try_into().expect("an 8-byte slice"))
}

pub fn write_u64(page: &mut [u8], offset: usize, value: u64) {
    page[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// The page id stored at `offset`. On disk "no page" is `-1` (`INVALID_PAGE_ID`), which `PageId::INVALID` is.
pub fn read_page_id(page: &[u8], offset: usize) -> PageId {
    PageId(i32::from_le_bytes(page[offset..offset + 4].try_into().expect("a 4-byte slice")))
}

pub fn write_page_id(page: &mut [u8], offset: usize, id: PageId) {
    page[offset..offset + 4].copy_from_slice(&id.0.to_le_bytes());
}

/// The page id at `offset`, or `None` if it holds `INVALID`.
pub fn read_optional_page_id(page: &[u8], offset: usize) -> Option<PageId> {
    Some(read_page_id(page, offset)).filter(|id| id.is_valid())
}

/// Stores `id`, or `INVALID` for `None`.
pub fn write_optional_page_id(page: &mut [u8], offset: usize, id: Option<PageId>) {
    write_page_id(page, offset, id.unwrap_or(PageId::INVALID));
}
