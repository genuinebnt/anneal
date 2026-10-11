//! The middle level of an extendible hash table. A directory has 2^global_depth slots; slot `hash & mask` holds the page id of the bucket for that
//! hash, and each slot records its bucket's **local depth** (how many bits of the hash that bucket really distinguishes). Several slots may share
//! one bucket.
//!
//! ```text
//! | max_depth u32 | global_depth u32 | local_depths [u8; 512] | bucket_page_ids [i32; 512] |
//! ```

// TODO(2b-02): your page type goes here: the table module names it, and what it stores and how is up to you.
