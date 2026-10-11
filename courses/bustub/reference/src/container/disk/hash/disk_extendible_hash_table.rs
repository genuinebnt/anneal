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

// @begin 2b-02
use crate::storage::page::extendible_htable_bucket_page::ExtendibleHTableBucketPage as Bucket;
use crate::storage::page::extendible_htable_directory_page::ExtendibleHTableDirectoryPage as Directory;
use crate::storage::page::extendible_htable_header_page::ExtendibleHTableHeaderPage as Header;
//~ // TODO(2b-02): your imports go here (your header, directory and bucket pages).
// @end

pub struct DiskExtendibleHashTable<'a, K, V, C> {
    // @begin 2b-02
    index_name: String,
    bpm: &'a BufferPoolManager,
    cmp: C,
    hash_fn: HashFunction<K>,
    header_max_depth: u32,
    directory_max_depth: u32,
    bucket_max_size: u32,
    header_page_id: PageId,
    _value: std::marker::PhantomData<V>,
    //~ _table: std::marker::PhantomData<(&'a (), K, V, C)>,
    //~ // TODO(2b-02): the fields are yours: the pool, the comparator, the hash function, the depth limits and the header page.
    // @end
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
        // @begin 2b-02
        let header_page_id = bpm.new_page();
        Header::new(&mut bpm.write_page(header_page_id)[..]).init(header_max_depth);
        DiskExtendibleHashTable {
            index_name: index_name.to_owned(),
            bpm,
            cmp,
            hash_fn,
            header_max_depth,
            directory_max_depth,
            bucket_max_size,
            header_page_id,
            _value: std::marker::PhantomData,
        }
        //~ todo!("2b-02: allocate a page for the header and format it (Header::init); remember its id and the parameters")
        // @end
    }

    pub fn default_header_max_depth() -> u32 {
        HTABLE_HEADER_MAX_DEPTH
    }

    pub fn default_directory_max_depth() -> u32 {
        HTABLE_DIRECTORY_MAX_DEPTH
    }

    /// As many pairs as fit in one bucket page.
    pub fn default_bucket_max_size() -> u32 {
        // @begin 2b-02
        Bucket::<&[u8], K, V>::capacity() as u32
        //~ todo!("2b-02: the number of pairs that fit in one of your bucket pages")
        // @end
    }

    /// The page id of the table's header page.
    pub fn get_header_page_id(&self) -> PageId {
        // @begin 2b-02
        self.header_page_id
        //~ todo!("2b-02: the id of the header page")
        // @end
    }

    /// The name the table was created with.
    pub fn index_name(&self) -> &str {
        // @begin 2b-02
        &self.index_name
        //~ todo!("2b-02: the name the table was created with")
        // @end
    }

    // @begin 2b-02
    /// The 32 bits of hash the table works with: the low half of the 64-bit hash (BusTub: `static_cast<uint32_t>`).
    fn hash(&self, key: &K) -> u32 {
        self.hash_fn.get_hash(key) as u32
    }
    // @end

    /// The global depth of the directory that serves `key`'s header slot (its number of slots is `2^depth`), or `None` if that slot has
    /// no directory yet. A read-only observer so that growth and shrinking can be checked without looking inside your pages.
    pub fn global_depth(&self, key: &K) -> Option<u32> {
        // @begin 2b-02
        let hash = self.hash(key);
        let header_guard = self.bpm.read_page(self.header_page_id);
        let header = Header::new(&header_guard[..]);
        let directory_page_id = header.get_directory_page_id(header.hash_to_directory_index(hash));
        if !directory_page_id.is_valid() {
            return None;
        }
        let directory_guard = self.bpm.read_page(directory_page_id);
        drop(header_guard);
        Some(Directory::new(&directory_guard[..]).get_global_depth())
        //~ todo!("2b-02: follow the header to the key's directory, if there is one, and report its global depth")
        // @end
    }

    /// The value stored for `key`: empty or one element, since keys are unique.
    pub fn get_value(&self, key: &K) -> Vec<V> {
        // @begin 2b-02
        let hash = self.hash(key);
        let header_guard = self.bpm.read_page(self.header_page_id);
        let header = Header::new(&header_guard[..]);
        let directory_page_id = header.get_directory_page_id(header.hash_to_directory_index(hash));
        if !directory_page_id.is_valid() {
            return Vec::new();
        }
        let directory_guard = self.bpm.read_page(directory_page_id);
        drop(header_guard); // the directory is latched: nobody can change the header's slot under us any more
        let directory = Directory::new(&directory_guard[..]);
        let bucket_page_id = directory.get_bucket_page_id(directory.hash_to_bucket_index(hash));
        let bucket_guard = self.bpm.read_page(bucket_page_id);
        drop(directory_guard);
        Bucket::<_, K, V>::new(&bucket_guard[..]).lookup(key, &self.cmp).into_iter().collect()
        //~ todo!("2b-02: hash the key, follow the header to a directory and the directory to a bucket (read-latching each), and look the key up; an empty slot means no value")
        // @end
    }

    /// Adds the pair. `false` if the key is already there, or the table can't grow any further to make room.
    pub fn insert(&self, key: &K, value: &V) -> bool {
        // @begin 2b-02
        let hash = self.hash(key);
        let mut header_guard = self.bpm.write_page(self.header_page_id);
        let (directory_idx, directory_page_id) = {
            let header = Header::new(&header_guard[..]);
            let idx = header.hash_to_directory_index(hash);
            (idx, header.get_directory_page_id(idx))
        };
        if !directory_page_id.is_valid() {
            return self.insert_to_new_directory(&mut header_guard[..], directory_idx, hash, key, value);
        }
        let mut directory_guard = self.bpm.write_page(directory_page_id);
        drop(header_guard);
        loop {
            let (bucket_idx, bucket_page_id) = {
                let directory = Directory::new(&directory_guard[..]);
                let idx = directory.hash_to_bucket_index(hash);
                (idx, directory.get_bucket_page_id(idx))
            };
            let mut bucket_guard = self.bpm.write_page(bucket_page_id);
            {
                let mut bucket = Bucket::<_, K, V>::new(&mut bucket_guard[..]);
                if bucket.lookup(key, &self.cmp).is_some() {
                    return false;
                }
                if !bucket.is_full() {
                    return bucket.insert(key, value, &self.cmp);
                }
            }
            // @begin 2b-03
            // The bucket is full: split it, then try again (the key may still land in a full bucket).
            let mut directory = Directory::new(&mut directory_guard[..]);
            if directory.get_local_depth(bucket_idx) == directory.get_global_depth() {
                if directory.get_global_depth() == directory.get_max_depth() {
                    return false; // the directory can't grow: the table is full here
                }
                directory.incr_global_depth();
            }
            self.split_bucket(&mut directory, &mut Bucket::<_, K, V>::new(&mut bucket_guard[..]), bucket_idx, bucket_page_id);
            //~ return false; // TODO(2b-03): split the full bucket (new bucket page, migrate entries, update the directory), then loop
            // @end
        }
        //~ todo!("2b-02: hash the key; follow the header to a directory and the directory to a bucket (write-latching each), creating the directory and bucket if there are none; insert if the key is new and there is room")
        // @end
    }

    // @begin 2b-02
    /// First key under this header slot: allocate a directory, then its first bucket, and put the pair there.
    fn insert_to_new_directory(&self, header: &mut [u8], directory_idx: u32, hash: u32, key: &K, value: &V) -> bool {
        let directory_page_id = self.bpm.new_page();
        let mut directory_guard = self.bpm.write_page(directory_page_id);
        Header::new(header).set_directory_page_id(directory_idx, directory_page_id);
        let mut directory = Directory::new(&mut directory_guard[..]);
        directory.init(self.directory_max_depth);
        let bucket_idx = directory.hash_to_bucket_index(hash);
        self.insert_to_new_bucket(&mut directory, bucket_idx, key, value)
    }
    // @end

    // @begin 2b-02
    /// First key for this directory slot: allocate a bucket (local depth 0), point the slot at it, and put the pair there.
    fn insert_to_new_bucket(&self, directory: &mut Directory<&mut [u8]>, bucket_idx: u32, key: &K, value: &V) -> bool {
        let bucket_page_id = self.bpm.new_page();
        let mut bucket_guard = self.bpm.write_page(bucket_page_id);
        let mut bucket = Bucket::<_, K, V>::new(&mut bucket_guard[..]);
        bucket.init(self.bucket_max_size);
        directory.set_bucket_page_id(bucket_idx, bucket_page_id);
        directory.set_local_depth(bucket_idx, 0);
        bucket.insert(key, value, &self.cmp)
    }
    // @end

    // @begin 2b-03
    /// Splits the full bucket in slot `bucket_idx` (one more hash bit now distinguishes it): a new bucket takes the entries whose
    /// new bit is 1, and the directory slots that pointed at the old bucket are divided between the two.
    fn split_bucket(&self, directory: &mut Directory<&mut [u8]>, old_bucket: &mut Bucket<&mut [u8], K, V>, bucket_idx: u32, old_page_id: PageId) {
        let new_depth = directory.get_local_depth(bucket_idx) + 1;
        let new_bit = 1u32 << (new_depth - 1);
        let new_page_id = self.bpm.new_page();
        let mut new_guard = self.bpm.write_page(new_page_id);
        let mut new_bucket = Bucket::<_, K, V>::new(&mut new_guard[..]);
        new_bucket.init(self.bucket_max_size);
        // Entries whose new bit is set move to the new bucket (walk backwards so removals don't shift what is left to visit).
        for i in (0..old_bucket.size()).rev() {
            let (k, v) = old_bucket.entry_at(i);
            if self.hash(&k) & new_bit != 0 {
                new_bucket.insert(&k, &v, &self.cmp);
                old_bucket.remove_at(i);
            }
        }
        // Every slot that pointed at the old bucket now has one more bit of depth; those with the new bit set point at the new one.
        for slot in 0..directory.size() {
            if directory.get_bucket_page_id(slot) == old_page_id {
                directory.set_local_depth(slot, new_depth as u8);
                if slot & new_bit != 0 {
                    directory.set_bucket_page_id(slot, new_page_id);
                }
            }
        }
    }
    // @end

    /// Removes `key`. `false` if it wasn't there.
    pub fn remove(&self, key: &K) -> bool {
        // @begin 2b-04
        let hash = self.hash(key);
        let header_guard = self.bpm.read_page(self.header_page_id);
        let header = Header::new(&header_guard[..]);
        let directory_page_id = header.get_directory_page_id(header.hash_to_directory_index(hash));
        if !directory_page_id.is_valid() {
            return false;
        }
        let mut directory_guard = self.bpm.write_page(directory_page_id);
        drop(header_guard);
        let (bucket_idx, bucket_page_id) = {
            let directory = Directory::new(&directory_guard[..]);
            let idx = directory.hash_to_bucket_index(hash);
            (idx, directory.get_bucket_page_id(idx))
        };
        let mut bucket_guard = self.bpm.write_page(bucket_page_id);
        let (removed, now_empty) = {
            let mut bucket = Bucket::<_, K, V>::new(&mut bucket_guard[..]);
            let removed = bucket.remove(key, &self.cmp);
            (removed, bucket.is_empty())
        };
        if removed && now_empty {
            drop(bucket_guard);
            self.merge_empty_buckets(&mut directory_guard[..], bucket_idx);
        }
        removed
        //~ todo!("2b-04: find the directory and the bucket (write-latched), remove the key from the bucket; say whether it was there")
        // @end
    }

    // @begin 2b-04
    /// Merges the empty bucket in slot `bucket_idx` into its split image, as long as the two have the same local depth, repeating
    /// while the merged bucket is empty or its new split image is, and finally halves the directory as far as it can shrink.
    fn merge_empty_buckets(&self, directory_page: &mut [u8], mut bucket_idx: u32) {
        let mut directory = Directory::new(directory_page);
        loop {
            let depth = directory.get_local_depth(bucket_idx);
            if depth == 0 {
                break;
            }
            let image_idx = directory.get_split_image_index(bucket_idx);
            if directory.get_local_depth(image_idx) != depth {
                break; // the sibling has been split further: it can't absorb this one
            }
            let (page_id, image_page_id) = (directory.get_bucket_page_id(bucket_idx), directory.get_bucket_page_id(image_idx));
            let is_empty = |id: PageId| Bucket::<_, K, V>::new(&self.bpm.read_page(id)[..]).is_empty();
            let (this_empty, image_empty) = (is_empty(page_id), is_empty(image_page_id));
            if !this_empty && !image_empty {
                break;
            }
            // Keep the non-empty one (either, if both are empty); delete the other and point all its slots at the survivor.
            let (keep, drop_id) = if this_empty { (image_page_id, page_id) } else { (page_id, image_page_id) };
            for slot in 0..directory.size() {
                if directory.get_bucket_page_id(slot) == drop_id {
                    directory.set_bucket_page_id(slot, keep);
                }
            }
            for slot in 0..directory.size() {
                if directory.get_bucket_page_id(slot) == keep {
                    directory.decr_local_depth(slot);
                }
            }
            self.bpm.delete_page(drop_id);
            bucket_idx = (0..directory.size()).find(|&s| directory.get_bucket_page_id(s) == keep).expect("the survivor has a slot");
        }
        while directory.can_shrink() {
            directory.decr_global_depth();
        }
    }
    // @end

    /// Checks every directory's invariants (see `Directory::verify_integrity`). Panics if one fails.
    pub fn verify_integrity(&self) {
        // @begin 2b-02
        let header_guard = self.bpm.read_page(self.header_page_id);
        let header = Header::new(&header_guard[..]);
        for i in 0..header.max_size() {
            let id = header.get_directory_page_id(i);
            if id.is_valid() {
                Directory::new(&self.bpm.read_page(id)[..]).verify_integrity();
            }
        }
        //~ todo!("2b-02: for every directory in the header, verify_integrity")
        // @end
    }
}
