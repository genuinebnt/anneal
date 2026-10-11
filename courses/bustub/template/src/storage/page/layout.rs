//! Where the fields of the extendible hash pages live inside a page, written down as constants and checked at compile time.
//!
//! The page views read and write by offset. Each layout is also described by a `repr(C)` struct with the same fields, and a mismatch between the
//! two descriptions is a compile error.

use std::mem::{offset_of, size_of};

use crate::common::config::BUSTUB_PAGE_SIZE;

/// log2 of the number of directory entries (512) a directory page and a header page hold.
pub const HTABLE_DIRECTORY_MAX_DEPTH: u32 = 9;
pub const HTABLE_DIRECTORY_ARRAY_SIZE: usize = 1 << HTABLE_DIRECTORY_MAX_DEPTH;
pub const HTABLE_HEADER_MAX_DEPTH: u32 = 9;
pub const HTABLE_HEADER_ARRAY_SIZE: usize = 1 << HTABLE_HEADER_MAX_DEPTH;
/// A bucket page starts with `size` and `max_size`, 4 bytes each.
pub const HTABLE_BUCKET_PAGE_METADATA_SIZE: usize = 8;

/// The directory page as a `repr(C)` struct: fields in order, padded for alignment.
#[repr(C)]
struct DirectoryPage {
    max_depth: u32,
    global_depth: u32,
    local_depths: [u8; HTABLE_DIRECTORY_ARRAY_SIZE],
    bucket_page_ids: [i32; HTABLE_DIRECTORY_ARRAY_SIZE],
}

/// The header page: 512 directory page ids, then the maximum depth.
#[repr(C)]
struct HeaderPage {
    directory_page_ids: [i32; HTABLE_HEADER_ARRAY_SIZE],
    max_depth: u32,
}

pub const DIRECTORY_MAX_DEPTH_OFFSET: usize = offset_of!(DirectoryPage, max_depth);
pub const DIRECTORY_GLOBAL_DEPTH_OFFSET: usize = offset_of!(DirectoryPage, global_depth);
pub const DIRECTORY_LOCAL_DEPTHS_OFFSET: usize = offset_of!(DirectoryPage, local_depths);
pub const DIRECTORY_BUCKET_PAGE_IDS_OFFSET: usize = offset_of!(DirectoryPage, bucket_page_ids);
pub const DIRECTORY_PAGE_SIZE: usize = size_of::<DirectoryPage>();

pub const HEADER_DIRECTORY_PAGE_IDS_OFFSET: usize = offset_of!(HeaderPage, directory_page_ids);
pub const HEADER_MAX_DEPTH_OFFSET: usize = offset_of!(HeaderPage, max_depth);
pub const HEADER_PAGE_SIZE: usize = size_of::<HeaderPage>();

// BusTub: static_assert(sizeof(ExtendibleHTableDirectoryPage) <= BUSTUB_PAGE_SIZE); the Rust spelling is a const assertion.
const _: () = assert!(DIRECTORY_PAGE_SIZE <= BUSTUB_PAGE_SIZE);
const _: () = assert!(HEADER_PAGE_SIZE <= BUSTUB_PAGE_SIZE);
