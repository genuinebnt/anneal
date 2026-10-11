//! An extendible hash table whose header, directories and buckets are pages in the buffer pool.
//!
//! Three levels: the header maps the top bits of a hash to a directory, a directory maps the low bits to a bucket, and a bucket holds a
//! few pairs. A full bucket splits, and the directory doubles if the bucket was already as deep as the directory; an empty one merges
//! with its split image, and the directory halves if it can.
//!
//! Pages are latched top down through guards, releasing a parent once its child is known to be safe: `get_value` read-latches header,
//! directory and bucket; `insert` and `remove` write-latch them.

use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::common::config::PageId;
use crate::container::hash::hash_function::HashFunction;
use crate::storage::index::fixed_size::FixedSize;
use crate::storage::index::generic_key::KeyComparator;
use crate::storage::page::layout::{HTABLE_DIRECTORY_MAX_DEPTH, HTABLE_HEADER_MAX_DEPTH};

// TODO(2b-02): your imports go here (your header, directory and bucket pages).

pub struct DiskExtendibleHashTable<'a, K, V, C> {
    _table: std::marker::PhantomData<(&'a (), K, V, C)>,
    // TODO(2b-02): the fields are yours: the pool, the comparator, the hash function, the depth limits and the header page.
}

impl<'a, K, V, C> DiskExtendibleHashTable<'a, K, V, C>
where
    K: FixedSize + Clone,
    V: FixedSize + Clone,
    C: KeyComparator<K>,
{
    /// Creates an empty table: allocates and formats the header page. `bucket_max_size` is how many pairs a bucket holds before it
    /// splits (use [`DiskExtendibleHashTable::default_bucket_max_size`] for "as many as fit in a page").
    pub fn new(
        index_name: &str,
        bpm: &'a BufferPoolManager,
        cmp: C,
        hash_fn: HashFunction<K>,
        header_max_depth: u32,
        directory_max_depth: u32,
        bucket_max_size: u32,
    ) -> Self {
        todo!("2b-02: allocate a page for the header and format it (Header::init); remember its id and the parameters")
    }

    pub fn default_header_max_depth() -> u32 {
        HTABLE_HEADER_MAX_DEPTH
    }

    pub fn default_directory_max_depth() -> u32 {
        HTABLE_DIRECTORY_MAX_DEPTH
    }

    /// As many pairs as fit in one bucket page.
    pub fn default_bucket_max_size() -> u32 {
        todo!("2b-02: the number of pairs that fit in one of your bucket pages")
    }

    /// The page id of the table's header page.
    pub fn get_header_page_id(&self) -> PageId {
        todo!("2b-02: the id of the header page")
    }

    /// The name the table was created with.
    pub fn index_name(&self) -> &str {
        todo!("2b-02: the name the table was created with")
    }


    /// The global depth of the directory that serves `key`'s header slot (its number of slots is `2^depth`), or `None` if that slot has
    /// no directory yet. A read-only observer so that growth and shrinking can be checked without looking inside the pages.
    pub fn global_depth(&self, key: &K) -> Option<u32> {
        todo!("2b-02: follow the header to the key's directory, if there is one, and report its global depth")
    }

    /// The value stored for `key`: empty or one element, since keys are unique.
    pub fn get_value(&self, key: &K) -> Vec<V> {
        todo!("2b-02: the key's bucket is found through the header and the directory, read-latched on the way; an empty slot means no value")
    }

    /// Adds the pair. `false` if the key is already there, or the table can't grow any further to make room.
    pub fn insert(&self, key: &K, value: &V) -> bool {
        todo!("2b-02: the key's bucket is found through the header and the directory (write-latched, created when missing); inserted if the key is new and there is room")
    }




    /// Removes `key`. `false` if it wasn't there.
    pub fn remove(&self, key: &K) -> bool {
        todo!("2b-04: find the directory and the bucket (write-latched), remove the key from the bucket; say whether it was there")
    }


    /// Checks every directory's invariants (see `Directory::verify_integrity`). Panics if one fails.
    pub fn verify_integrity(&self) {
        todo!("2b-02: for every directory in the header, verify_integrity")
    }
}
