//! The middle level of an extendible hash table. A directory has 2^global_depth slots; slot `hash & mask` holds the page id of the bucket for that
//! hash, and each slot records its bucket's **local depth** (how many bits of the hash that bucket really distinguishes). Several slots may share
//! one bucket.
//!
//! ```text
//! | max_depth u32 | global_depth u32 | local_depths [u8; 512] | bucket_page_ids [i32; 512] |
//! ```

// @begin 2b-02
use std::collections::HashMap;

use super::layout::*;
use super::page_bytes::*;
use crate::common::config::PageId;

pub struct ExtendibleHTableDirectoryPage<B> {
    page: B,
}

impl<B> ExtendibleHTableDirectoryPage<B> {
    pub fn new(page: B) -> ExtendibleHTableDirectoryPage<B> {
        ExtendibleHTableDirectoryPage { page }
    }
}

impl<B: AsRef<[u8]>> ExtendibleHTableDirectoryPage<B> {
    pub fn get_max_depth(&self) -> u32 {
        read_u32(self.page.as_ref(), DIRECTORY_MAX_DEPTH_OFFSET)
    }

    pub fn get_global_depth(&self) -> u32 {
        read_u32(self.page.as_ref(), DIRECTORY_GLOBAL_DEPTH_OFFSET)
    }

    /// The number of slots in use: 2^global_depth.
    pub fn size(&self) -> u32 {
        1 << self.get_global_depth()
    }

    /// The most slots the directory may ever have: 2^max_depth.
    pub fn max_size(&self) -> u32 {
        1 << self.get_max_depth()
    }

    pub fn get_bucket_page_id(&self, bucket_idx: u32) -> PageId {
        assert!(bucket_idx < self.max_size(), "slot {bucket_idx} is out of range");
        read_page_id(self.page.as_ref(), DIRECTORY_BUCKET_PAGE_IDS_OFFSET + 4 * bucket_idx as usize)
    }

    pub fn get_local_depth(&self, bucket_idx: u32) -> u32 {
        assert!(bucket_idx < self.max_size(), "slot {bucket_idx} is out of range");
        self.page.as_ref()[DIRECTORY_LOCAL_DEPTHS_OFFSET + bucket_idx as usize] as u32
    }

    /// A mask of `global_depth` ones from the low end: `0b111` for depth 3.
    pub fn get_global_depth_mask(&self) -> u32 {
        (1u32 << self.get_global_depth()) - 1
    }

    /// The same for the local depth of the bucket in slot `bucket_idx`.
    pub fn get_local_depth_mask(&self, bucket_idx: u32) -> u32 {
        (1u32 << self.get_local_depth(bucket_idx)) - 1
    }

    /// The slot a hash maps to: its low `global_depth` bits.
    pub fn hash_to_bucket_index(&self, hash: u32) -> u32 {
        hash & self.get_global_depth_mask()
    }

    /// The slot of the bucket this bucket would split from or merge with: flip the highest bit its local depth distinguishes.
    /// A bucket of local depth 0 has no sibling and is its own split image.
    pub fn get_split_image_index(&self, bucket_idx: u32) -> u32 {
        match self.get_local_depth(bucket_idx) {
            0 => bucket_idx,
            depth => bucket_idx ^ (1 << (depth - 1)),
        }
    }

    /// True if no bucket uses all global_depth bits, so halving the directory loses nothing.
    pub fn can_shrink(&self) -> bool {
        let global = self.get_global_depth();
        global > 0 && (0..self.size()).all(|i| self.get_local_depth(i) < global)
    }

    /// Checks the invariants: (1) every local depth <= the global depth; (2) a bucket with local depth d is pointed to by exactly
    /// 2^(global - d) slots; (3) all slots with the same bucket page id have the same local depth. Panics if one fails.
    pub fn verify_integrity(&self) {
        let global = self.get_global_depth();
        let mut pointers: HashMap<PageId, (u32, u32)> = HashMap::new(); // page id -> (local depth, how many slots)
        for i in 0..self.size() {
            let (page_id, depth) = (self.get_bucket_page_id(i), self.get_local_depth(i));
            assert!(depth <= global, "slot {i}: local depth {depth} is above the global depth {global}");
            let entry = pointers.entry(page_id).or_insert((depth, 0));
            assert_eq!(entry.0, depth, "slot {i}: bucket {} appears with two different local depths", page_id.0);
            entry.1 += 1;
        }
        for (page_id, (depth, count)) in pointers {
            assert_eq!(count, 1 << (global - depth), "bucket {} with local depth {depth} must have 2^({global} - {depth}) slots, has {count}", page_id.0);
        }
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> ExtendibleHTableDirectoryPage<B> {
    /// Formats a fresh page: the given max depth (at most 9), global depth 0, every local depth 0, every bucket id `INVALID`.
    pub fn init(&mut self, max_depth: u32) {
        assert!(max_depth <= HTABLE_DIRECTORY_MAX_DEPTH, "max depth {max_depth} does not fit in a directory page");
        let page = self.page.as_mut();
        write_u32(page, DIRECTORY_MAX_DEPTH_OFFSET, max_depth);
        write_u32(page, DIRECTORY_GLOBAL_DEPTH_OFFSET, 0);
        page[DIRECTORY_LOCAL_DEPTHS_OFFSET..DIRECTORY_LOCAL_DEPTHS_OFFSET + HTABLE_DIRECTORY_ARRAY_SIZE].fill(0);
        for slot in 0..HTABLE_DIRECTORY_ARRAY_SIZE {
            write_page_id(page, DIRECTORY_BUCKET_PAGE_IDS_OFFSET + 4 * slot, PageId::INVALID);
        }
    }

    pub fn set_bucket_page_id(&mut self, bucket_idx: u32, bucket_page_id: PageId) {
        assert!(bucket_idx < self.max_size(), "slot {bucket_idx} is out of range");
        write_page_id(self.page.as_mut(), DIRECTORY_BUCKET_PAGE_IDS_OFFSET + 4 * bucket_idx as usize, bucket_page_id);
    }

    pub fn set_local_depth(&mut self, bucket_idx: u32, local_depth: u8) {
        assert!(bucket_idx < self.max_size(), "slot {bucket_idx} is out of range");
        assert!(local_depth as u32 <= self.get_max_depth(), "local depth {local_depth} is above the max depth");
        self.page.as_mut()[DIRECTORY_LOCAL_DEPTHS_OFFSET + bucket_idx as usize] = local_depth;
    }

    pub fn incr_local_depth(&mut self, bucket_idx: u32) {
        let depth = self.get_local_depth(bucket_idx);
        assert!(depth < self.get_max_depth(), "local depth is already at the max depth");
        self.set_local_depth(bucket_idx, (depth + 1) as u8);
    }

    pub fn decr_local_depth(&mut self, bucket_idx: u32) {
        let depth = self.get_local_depth(bucket_idx);
        assert!(depth > 0, "local depth is already 0");
        self.set_local_depth(bucket_idx, (depth - 1) as u8);
    }

    /// Doubles the directory: the new upper half of the slots is a copy of the lower half (bucket ids and local depths), because
    /// until a bucket splits, a hash with the new top bit set still goes to the same bucket. Panics at the max depth.
    pub fn incr_global_depth(&mut self) {
        let global = self.get_global_depth();
        assert!(global < self.get_max_depth(), "the directory is already at its max depth");
        let size = 1usize << global;
        for i in 0..size {
            let (id, depth) = (self.get_bucket_page_id(i as u32), self.get_local_depth(i as u32));
            self.page.as_mut()[DIRECTORY_LOCAL_DEPTHS_OFFSET + size + i] = depth as u8;
            write_page_id(self.page.as_mut(), DIRECTORY_BUCKET_PAGE_IDS_OFFSET + 4 * (size + i), id);
        }
        write_u32(self.page.as_mut(), DIRECTORY_GLOBAL_DEPTH_OFFSET, global + 1);
    }

    /// Halves the directory. The caller has checked `can_shrink`.
    pub fn decr_global_depth(&mut self) {
        let global = self.get_global_depth();
        assert!(global > 0, "the directory is already at depth 0");
        write_u32(self.page.as_mut(), DIRECTORY_GLOBAL_DEPTH_OFFSET, global - 1);
    }
}
//~ // TODO(2b-02): your page type goes here: the table module names it, and what it stores and how is up to you.
// @end
