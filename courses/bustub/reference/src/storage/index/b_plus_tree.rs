//! A B+ tree index whose nodes are pages in the buffer pool. Internal pages direct the search; leaf pages hold the `(key, value)` pairs and are
//! chained left to right for range scans. Keys are unique.
//!
//! A node is split when it overflows (a leaf the moment it holds `leaf_max_size` pairs, an internal page when it would need more than
//! `internal_max_size` children) and merged or topped up from a sibling when it underflows, so the tree grows and shrinks at the root and every
//! leaf is at the same depth.
//!
//! Latching is by guards, top down. Readers hold a parent until the child is latched ("latch crabbing"). Writers keep write guards on the whole
//! path (`Context::write_set`) until a child that cannot split or underflow is reached ("safe"), which lets them let go of everything above it;
//! and most writes try an optimistic path first that write-latches only the leaf.

use std::cmp::Ordering;
use std::marker::PhantomData;

use super::index_iterator::IndexIterator;
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::buffer::traced_buffer_pool_manager::TracedBufferPoolManager;
use crate::common::config::PageId;
use crate::storage::index::fixed_size::FixedSize;
use crate::storage::index::generic_key::KeyComparator;

// @begin 2c-01
use crate::storage::page::b_plus_tree_header_page::BPlusTreeHeaderPage as Header;
use crate::storage::page::b_plus_tree_internal_page::BPlusTreeInternalPage as Internal;
use crate::storage::page::b_plus_tree_leaf_page::BPlusTreeLeafPage as Leaf;
use crate::storage::page::b_plus_tree_page::BPlusTreePage as Page;
use crate::storage::page::page_guard::{ReadPageGuard, WritePageGuard};
//~ // TODO(2c-01): your imports go here (your page types and the guards).
// @end

// @begin 2c-01
/// What a write operation keeps latched while it works: the header page (as long as the root might change), and the write guards
/// of the pages from the top of the part of the tree it may modify down to the current one. BusTub's `Context`.
struct Context<'a> {
    header_page: Option<WritePageGuard<'a>>,
    root_page_id: PageId,
    write_set: Vec<WritePageGuard<'a>>,
}

impl<'a> Context<'a> {
    fn new(header_page: WritePageGuard<'a>) -> Context<'a> {
        Context { header_page: Some(header_page), root_page_id: PageId::INVALID, write_set: Vec::new() }
    }

    fn is_root_page(&self, page_id: PageId) -> bool {
        page_id == self.root_page_id
    }

    /// Lets go of everything latched so far: the header and every page on the path.
    fn release_ancestors(&mut self) {
        self.header_page = None;
        self.write_set.clear();
    }

    fn set_root(&mut self, root_page_id: PageId) {
        let header = self.header_page.as_mut().expect("the root can only change while the header page is latched");
        Header::new(&mut header[..]).set_root_page_id(root_page_id);
    }
}
//~ // TODO(2c-01): private types of yours go here (a write operation needs to remember the guards it holds).
// @end

/// A B+ tree over the buffer pool. `TOMBS` is the size of each leaf's tombstone buffer; 0 means deletes are physical.
pub struct BPlusTree<'a, K, V, C, const TOMBS: usize = 0> {
    /// The buffer pool the tree reads and writes through. It is public so that the pages the tree latched can be counted; every page access goes
    /// through it.
    pub bpm: TracedBufferPoolManager<'a>,
    // @begin 2c-01
    index_name: String,
    cmp: C,
    leaf_max_size: u32,
    internal_max_size: u32,
    header_page_id: PageId,
    _entry: PhantomData<(K, V)>,
    //~ _tree: PhantomData<(K, V, C)>,
    //~ // TODO(2c-01): the fields are yours: the name, the comparator, the two sizes and the header page's id.
    // @end
}

impl<'a, K, V, C, const TOMBS: usize> BPlusTree<'a, K, V, C, TOMBS>
where
    K: FixedSize + Clone,
    V: FixedSize + Clone,
    C: KeyComparator<K>,
{
    /// Creates an empty tree whose header page is `header_page_id` (the caller allocated it) and formats that page.
    /// A leaf holds at most `leaf_max_size` pairs (at least 2), an internal page at most `internal_max_size` children (at least 3).
    pub fn new(index_name: &str, header_page_id: PageId, bpm: &'a BufferPoolManager, cmp: C, leaf_max_size: u32, internal_max_size: u32) -> Self {
        // @begin 2c-01
        assert!(leaf_max_size >= 2 && leaf_max_size as usize <= Leaf::<&[u8], K, V, TOMBS>::capacity(), "leaf_max_size {leaf_max_size} is not usable");
        assert!(internal_max_size >= 3 && internal_max_size as usize <= Internal::<&[u8], K>::capacity(), "internal_max_size {internal_max_size} is not usable");
        let bpm = TracedBufferPoolManager::new(bpm);
        Header::new(&mut bpm.write_page(header_page_id)[..]).init();
        BPlusTree { index_name: index_name.to_owned(), bpm, cmp, leaf_max_size, internal_max_size, header_page_id, _entry: PhantomData }
        //~ todo!("2c-01: checks the sizes are usable, remembers the parameters and formats the header page as an empty tree")
        // @end
    }

    /// As many pairs as fit in a leaf page.
    pub fn default_leaf_max_size() -> u32 {
        // @begin 2c-01
        Leaf::<&[u8], K, V, TOMBS>::capacity() as u32
        //~ todo!("2c-01: how many pairs fit in one of your leaf pages")
        // @end
    }

    /// As many children as fit in an internal page.
    pub fn default_internal_max_size() -> u32 {
        // @begin 2c-01
        Internal::<&[u8], K>::capacity() as u32
        //~ todo!("2c-01: how many children fit in one of your internal pages")
        // @end
    }

    /// The name the tree was created with.
    pub fn index_name(&self) -> &str {
        // @begin 2c-01
        &self.index_name
        //~ todo!("2c-01: the name the tree was created with")
        // @end
    }

    /// Whether the tree has no root.
    pub fn is_empty(&self) -> bool {
        // @begin 2c-01
        !self.get_root_page_id().is_valid()
        //~ todo!("2c-01: the tree is empty when the header names no root")
        // @end
    }

    /// The root's page id; `PageId::INVALID` for an empty tree.
    pub fn get_root_page_id(&self) -> PageId {
        // @begin 2c-01
        Header::new(&self.bpm.read_page(self.header_page_id)[..]).root_page_id()
        //~ todo!("2c-01: read the root id from the header page (under its read latch)")
        // @end
    }

    /// How many levels the tree has: 0 for an empty tree, 1 for a lone root leaf. Read-only.
    pub fn depth(&self) -> usize {
        // @begin 2c-01
        let header_guard = self.bpm.read_page(self.header_page_id);
        let root = Header::new(&header_guard[..]).root_page_id();
        if !root.is_valid() {
            return 0;
        }
        let mut guard = self.bpm.read_page(root);
        drop(header_guard);
        let mut levels = 1;
        while !Page::new(&guard[..]).is_leaf_page() {
            let child = Internal::<_, K>::new(&guard[..]).value_at(0);
            guard = self.bpm.read_page(child);
            levels += 1;
        }
        levels
        //~ todo!("2c-01: 0 for an empty tree; otherwise follow the first child from the root down to a leaf and count the levels")
        // @end
    }

    /// How many `(key, value)` pairs each leaf holds physically (live or, with tombstones, deleted), from the leftmost leaf to the rightmost,
    /// following the links between leaves. Empty for an empty tree. Read-only.
    pub fn leaf_sizes(&self) -> Vec<usize> {
        // @begin 2c-01
        let mut sizes = Vec::new();
        let mut next = self.find_leaf(None).map(|g| g.get_page_id());
        while let Some(page_id) = next {
            let guard = self.bpm.read_page(page_id);
            let leaf = Leaf::<_, K, V, TOMBS>::new(&guard[..]);
            sizes.push(leaf.size() as usize);
            next = leaf.next_page_id();
        }
        sizes
        //~ todo!("2c-01: walk the leaves left to right (the leaf links) and collect how many pairs each holds")
        // @end
    }

    /// The keys physically stored in each leaf, in order, tombstoned ones included, from the leftmost leaf to the rightmost. Read-only.
    pub fn leaf_keys(&self) -> Vec<Vec<K>> {
        // @begin 2d-01
        let mut all = Vec::new();
        let mut next = self.find_leaf(None).map(|g| g.get_page_id());
        while let Some(page_id) = next {
            let guard = self.bpm.read_page(page_id);
            let leaf = Leaf::<_, K, V, TOMBS>::new(&guard[..]);
            all.push((0..leaf.size()).map(|i| leaf.key_at(i)).collect());
            next = leaf.next_page_id();
        }
        all
        //~ Vec::new() // TODO(2d-01): the keys of every leaf, tombstoned ones included, left to right
        // @end
    }

    /// The keys in each leaf's tombstone buffer, oldest first, for every leaf from left to right. Empty lists when `TOMBS` is 0. Read-only.
    pub fn leaf_tombstones(&self) -> Vec<Vec<K>> {
        // @begin 2d-01
        let mut all = Vec::new();
        let mut next = self.find_leaf(None).map(|g| g.get_page_id());
        while let Some(page_id) = next {
            let guard = self.bpm.read_page(page_id);
            let leaf = Leaf::<_, K, V, TOMBS>::new(&guard[..]);
            all.push(leaf.tombstones());
            next = leaf.next_page_id();
        }
        all
        //~ Vec::new() // TODO(2d-01): the tombstone buffer of every leaf, left to right
        // @end
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // Search
    // ------------------------------------------------------------------------------------------------------------------------

    // @begin 2c-01
    /// The leaf that holds `key` (or would), or the leftmost leaf if `key` is `None`; `None` for an empty tree. Read-latches down
    /// the tree, holding each page until its child is latched.
    fn find_leaf(&self, key: Option<&K>) -> Option<ReadPageGuard<'a>> {
        let header_guard = self.bpm.read_page(self.header_page_id);
        let root = Header::new(&header_guard[..]).root_page_id();
        if !root.is_valid() {
            return None;
        }
        let mut guard = self.bpm.read_page(root);
        drop(header_guard);
        while !Page::new(&guard[..]).is_leaf_page() {
            let internal = Internal::<_, K>::new(&guard[..]);
            let child = match key {
                Some(key) => internal.child_for(key, &self.cmp),
                None => internal.value_at(0),
            };
            guard = self.bpm.read_page(child); // the child is latched before the parent guard is dropped by this assignment
        }
        Some(guard)
    }
    //~ // TODO(2c-01): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    /// The value stored for `key`: empty or one element, since keys are unique.
    pub fn get_value(&self, key: &K) -> Vec<V> {
        // @begin 2c-01
        let Some(leaf_guard) = self.find_leaf(Some(key)) else { return Vec::new() };
        Leaf::<_, K, V, TOMBS>::new(&leaf_guard[..]).lookup(key, &self.cmp).into_iter().collect()
        //~ todo!("2c-01: find the leaf and look the key up in it")
        // @end
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // Insert
    // ------------------------------------------------------------------------------------------------------------------------

    /// Adds the pair. `false` (and nothing changes) if the key is already in the tree.
    pub fn insert(&self, key: &K, value: &V) -> bool {
        // @begin 2c-01
        // @begin 2c-05
        if let Some(inserted) = self.insert_optimistic(key, value) {
            return inserted;
        }
        //~ // TODO(2c-02): try the optimistic path first: write-latch only the leaf, and do it all there if the leaf has room
        // @end
        let mut ctx = Context::new(self.bpm.write_page(self.header_page_id));
        let root = Header::new(&ctx.header_page.as_ref().unwrap()[..]).root_page_id();
        if !root.is_valid() {
            // An empty tree: the first pair goes into a new leaf, which is the root.
            let root_page_id = self.bpm.new_page();
            let mut root_guard = self.bpm.write_page(root_page_id);
            let mut leaf = Leaf::<_, K, V, TOMBS>::new(&mut root_guard[..]);
            leaf.init(self.leaf_max_size);
            leaf.insert(key, value, &self.cmp);
            ctx.set_root(root_page_id);
            return true;
        }
        ctx.root_page_id = root;
        // @begin 2c-05
        self.descend_for_write(&mut ctx, key, |this, page, _| this.safe_to_insert(page));
        //~ self.descend_for_write(&mut ctx, key, |_, _, _| false); // TODO(2c-05): nothing is safe yet: keep every guard on the path
        // @end
        let mut leaf_guard = ctx.write_set.pop().expect("the descent ends at a leaf");
        let leaf_page_id = leaf_guard.get_page_id();
        let mut leaf = Leaf::<_, K, V, TOMBS>::new(&mut leaf_guard[..]);
        if !leaf.insert(key, value, &self.cmp) {
            return false; // the key is already there
        }
        // @begin 2c-02
        if leaf.size() < leaf.max_size() {
            return true;
        }
        // The leaf is full: move its upper half to a new leaf to its right and give the parent the new leaf's first key.
        let new_page_id = self.bpm.new_page();
        let mut new_guard = self.bpm.write_page(new_page_id);
        let mut right = Leaf::<_, K, V, TOMBS>::new(&mut new_guard[..]);
        right.init(leaf.max_size());
        let size = leaf.size();
        let keep = size.div_ceil(2);
        // @begin 2d-02
        // the tombstones go with their pairs, in the same order
        let (stay, go): (Vec<K>, Vec<K>) = leaf.tombstones().into_iter().partition(|t| leaf.lower_bound(t, &self.cmp) < keep);
        //~ // TODO(2d-01): with tombstones, split the buffer too: the keys of the upper half go to the new leaf, the order kept
        // @end
        for i in keep..size {
            let (k, v) = leaf.entry_at(i);
            right.set_entry_at(i - keep, &k, &v);
        }
        right.set_size(size - keep);
        leaf.set_size(keep);
        // @begin 2d-02
        leaf.set_tombstones(&stay);
        right.set_tombstones(&go);
        //~ // TODO(2d-01): store the two halves of the buffer
        // @end
        right.set_next_page_id(leaf.next_page_id());
        leaf.set_next_page_id(Some(new_page_id));
        let separator = right.key_at(0);
        drop(new_guard);
        self.insert_into_parent(&mut ctx, leaf_page_id, separator, new_page_id);
        //~ // TODO(2c-01): a leaf that has reached max_size splits: upper half to a new leaf, next pointers, then insert_into_parent
        // @end
        true
        //~ todo!("2c-01: latch the header; an empty tree gets a root leaf; otherwise latch down to the leaf (descend_for_write) and insert there")
        // @end
    }

    // @begin 2c-01
    /// Latches `key`'s path from the root down, pushing each page's write guard on `ctx.write_set`. `safe(self, page, is_root)` says
    /// whether the operation cannot change anything above this page; from the first such page down, nothing above it is needed.
    fn descend_for_write(&self, ctx: &mut Context<'a>, key: &K, safe: impl Fn(&Self, &[u8], bool) -> bool) {
        let mut page_id = ctx.root_page_id;
        loop {
            let guard = self.bpm.write_page(page_id);
            let next = (!Page::new(&guard[..]).is_leaf_page()).then(|| Internal::<_, K>::new(&guard[..]).child_for(key, &self.cmp));
            // @begin 2c-05
            if safe(self, &guard[..], ctx.is_root_page(page_id)) {
                ctx.release_ancestors();
            }
            //~ // TODO(2c-02): if this page is safe (it cannot split or underflow), release everything above it: ctx.release_ancestors()
            // @end
            ctx.write_set.push(guard);
            match next {
                Some(child) => page_id = child,
                None => return,
            }
        }
    }
    //~ // TODO(2c-01): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // @begin 2c-05
    /// A page is safe for an insert when adding one pair (or child) cannot make it split.
    fn safe_to_insert(&self, page: &[u8]) -> bool {
        let p = Page::new(page);
        if p.is_leaf_page() {
            p.size() + 1 < p.max_size()
        } else {
            p.size() < p.max_size()
        }
    }
    //~ // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // @begin 2c-02
    /// After a split: the node `left_page_id` (still write-latched) has a new right sibling `right_page_id` whose smallest key is
    /// `key`. Records that in the parent, splitting the parent too if it is full, and growing a new root if the split reached the top.
    fn insert_into_parent(&self, ctx: &mut Context<'a>, mut left_page_id: PageId, mut key: K, mut right_page_id: PageId) {
        loop {
            let Some(mut parent_guard) = ctx.write_set.pop() else {
                // The node that split was the root: a new root with the two halves as children.
                let root_page_id = self.bpm.new_page();
                let mut root_guard = self.bpm.write_page(root_page_id);
                let mut root = Internal::<_, K>::new(&mut root_guard[..]);
                root.init(self.internal_max_size);
                root.set_entry_at(0, &key, left_page_id); // slot 0's key is never read
                root.set_entry_at(1, &key, right_page_id);
                root.set_size(2);
                drop(root_guard);
                ctx.set_root(root_page_id);
                return;
            };
            let parent_page_id = parent_guard.get_page_id();
            let mut parent = Internal::<_, K>::new(&mut parent_guard[..]);
            if parent.size() < parent.max_size() {
                parent.insert_child(&key, right_page_id, &self.cmp);
                return;
            }
            // The parent is full as well: spread its children, plus the new one, over two pages. The first key of the right half
            // moves up (it separates the halves) instead of staying in either of them.
            let mut entries: Vec<(K, PageId)> = (0..parent.size()).map(|i| parent.entry_at(i)).collect();
            let at = 1 + entries[1..].partition_point(|(k, _)| self.cmp.compare(k, &key).is_le());
            entries.insert(at, (key.clone(), right_page_id));
            let keep = entries.len().div_ceil(2);
            let new_page_id = self.bpm.new_page();
            let mut new_guard = self.bpm.write_page(new_page_id);
            let mut right = Internal::<_, K>::new(&mut new_guard[..]);
            right.init(parent.max_size());
            for (i, (k, child)) in entries[keep..].iter().enumerate() {
                right.set_entry_at(i as u32, k, *child);
            }
            right.set_size((entries.len() - keep) as u32);
            for (i, (k, child)) in entries[..keep].iter().enumerate() {
                parent.set_entry_at(i as u32, k, *child);
            }
            parent.set_size(keep as u32);
            drop(new_guard);
            left_page_id = parent_page_id;
            key = entries[keep].0.clone();
            right_page_id = new_page_id;
        }
    }
    //~ // TODO(2c-02): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // ------------------------------------------------------------------------------------------------------------------------
    // Remove
    // ------------------------------------------------------------------------------------------------------------------------

    /// Removes the pair with this key, if there is one.
    pub fn remove(&self, key: &K) {
        // @begin 2c-04
        // @begin 2c-05
        if self.remove_optimistic(key) {
            return;
        }
        //~ // TODO(2c-02): try the optimistic path first: write-latch only the leaf, and do it all there if the leaf stays at least half full
        // @end
        let mut to_delete = Vec::new();
        {
            let mut ctx = Context::new(self.bpm.write_page(self.header_page_id));
            let root = Header::new(&ctx.header_page.as_ref().unwrap()[..]).root_page_id();
            if !root.is_valid() {
                return;
            }
            ctx.root_page_id = root;
            // @begin 2c-05
            self.descend_for_write(&mut ctx, key, |this, page, is_root| this.safe_to_remove(page, is_root));
            //~ self.descend_for_write(&mut ctx, key, |_, _, _| false); // TODO(2c-05): nothing is safe yet: keep every guard on the path
            // @end
            let mut leaf_guard = ctx.write_set.pop().expect("the descent ends at a leaf");
            let leaf_page_id = leaf_guard.get_page_id();
            let mut leaf = Leaf::<_, K, V, TOMBS>::new(&mut leaf_guard[..]);
            // @begin 2d-01
            let removed = if TOMBS > 0 { leaf.remove_logically(key, &self.cmp) } else { leaf.remove(key, &self.cmp) };
            //~ let removed = leaf.remove(key, &self.cmp); // TODO(2d-01): with tombstones, delete logically: leaf.remove_logically
            // @end
            if !removed {
                return;
            }
            if ctx.is_root_page(leaf_page_id) {
                if leaf.size() == 0 {
                    // The last pair is gone: the tree is empty again.
                    ctx.set_root(PageId::INVALID);
                    to_delete.push(leaf_page_id);
                }
            } else if leaf.size() < leaf.min_size() {
                self.rebalance(&mut ctx, leaf_guard, &mut to_delete);
            }
        }
        // The guards are dropped: a page can only be deleted from the pool once nobody has it pinned.
        for page_id in to_delete {
            self.bpm.delete_page(page_id);
        }
        //~ todo!("2c-02: removes the key from its leaf; an emptied root leaf makes the tree empty, and a leaf below its minimum size is rebalanced")
        // @end
    }

    // @begin 2c-05
    /// A page is safe for a remove when taking one pair (or child) away cannot make it underflow, or empty the root.
    fn safe_to_remove(&self, page: &[u8], is_root: bool) -> bool {
        let p = Page::new(page);
        match (is_root, p.is_leaf_page()) {
            (true, true) => p.size() > 1,    // a root leaf may shrink to one pair, but not to none
            (true, false) => p.size() > 2,   // a root with two children must be replaced by its only child
            (false, _) => p.size() > p.min_size(),
        }
    }
    //~ // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // @begin 2c-04
    /// `node` (write-latched, not the root) has fewer than `min_size` entries. Borrow one from a sibling that has a spare; failing
    /// that, merge with a sibling and fix the parent, which may in turn underflow. Pages that are gone are added to `to_delete`.
    fn rebalance(&self, ctx: &mut Context<'a>, mut node_guard: WritePageGuard<'a>, to_delete: &mut Vec<PageId>) {
        loop {
            let node_page_id = node_guard.get_page_id();
            let is_leaf = Page::new(&node_guard[..]).is_leaf_page();
            let mut parent_guard = ctx.write_set.pop().expect("a page that is not the root has a latched parent");
            let parent_page_id = parent_guard.get_page_id();
            let (idx, parent_size) = {
                let parent = Internal::<_, K>::new(&parent_guard[..]);
                (parent.value_index(node_page_id).expect("the parent lists its child"), parent.size())
            };
            // @begin 2d-02
            // A short leaf first really removes its own tombstoned pairs: they are deleted anyway, and borrowing or merging should
            // move only live pairs (and the tombstones of the neighbour it takes from, which travel with their pairs).
            if is_leaf && TOMBS > 0 {
                Leaf::<_, K, V, TOMBS>::new(&mut node_guard[..]).purge_tombstones(&self.cmp);
            }
            //~ // TODO(2d-01): with tombstones, purge the short leaf's own tombstones before borrowing or merging
            // @end
            let sibling = |i: u32| self.bpm.write_page(Internal::<_, K>::new(&parent_guard[..]).value_at(i));
            let mut left_guard = (idx > 0).then(|| sibling(idx - 1));
            let mut right_guard = (idx + 1 < parent_size).then(|| sibling(idx + 1));
            let has_spare = |g: &WritePageGuard<'a>| {
                let p = Page::new(&g[..]);
                p.size() > p.min_size()
            };
            // Borrow until the node is back at min_size or no sibling has a spare entry (without tombstones one entry is always enough).
            loop {
                let node = Page::new(&node_guard[..]);
                if node.size() >= node.min_size() {
                    return;
                }
                if left_guard.as_ref().is_some_and(has_spare) {
                    self.borrow_from_left(&mut parent_guard, idx, left_guard.as_mut().unwrap(), &mut node_guard, is_leaf);
                } else if right_guard.as_ref().is_some_and(has_spare) {
                    self.borrow_from_right(&mut parent_guard, idx, &mut node_guard, right_guard.as_mut().unwrap(), is_leaf);
                } else {
                    break;
                }
            }
            // Neither sibling has a spare entry: merge with one of them. The pair of pages together fits in one page.
            let removed_idx = if let Some(mut left) = left_guard {
                self.merge(&mut parent_guard, idx, &mut left, &mut node_guard, is_leaf);
                to_delete.push(node_page_id);
                idx
            } else {
                let mut right = right_guard.expect("a page that is not the root has a sibling");
                self.merge(&mut parent_guard, idx + 1, &mut node_guard, &mut right, is_leaf);
                to_delete.push(right.get_page_id());
                idx + 1
            };
            let mut parent = Internal::<_, K>::new(&mut parent_guard[..]);
            parent.remove_at(removed_idx);
            if ctx.is_root_page(parent_page_id) {
                if parent.size() == 1 {
                    // The root has a single child left: that child is the new root and the tree is one level lower.
                    ctx.set_root(parent.value_at(0));
                    to_delete.push(parent_page_id);
                }
                return;
            }
            if parent.size() >= parent.min_size() {
                return;
            }
            node_guard = parent_guard; // the parent is short now: repeat one level up
        }
    }
    //~ // TODO(2c-04): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // @begin 2c-04
    /// Moves the last entry of `left` to the front of `node`; `idx` is `node`'s slot in `parent`, whose separator changes.
    fn borrow_from_left(&self, parent_guard: &mut WritePageGuard<'a>, idx: u32, left_guard: &mut WritePageGuard<'a>, node_guard: &mut WritePageGuard<'a>, is_leaf: bool) {
        let mut parent = Internal::<_, K>::new(&mut parent_guard[..]);
        if is_leaf {
            let mut left = Leaf::<_, K, V, TOMBS>::new(&mut left_guard[..]);
            let last = left.size() - 1;
            let (k, v) = left.entry_at(last);
            // @begin 2d-02
            let was_deleted = left.is_deleted_at(last);
            //~ let was_deleted = false; // TODO(2d-01): a tombstoned pair keeps its tombstone when it moves
            // @end
            left.remove_at(last);
            let mut node = Leaf::<_, K, V, TOMBS>::new(&mut node_guard[..]);
            node.insert_at_front(&k, &v);
            // @begin 2d-02
            if was_deleted {
                if node.tombstones().len() < TOMBS {
                    node.add_tombstone(&k);
                } else {
                    node.remove_at(0); // no room for another tombstone: the deleted pair is simply dropped
                }
            }
            //~ // TODO(2d-01): re-buffer the tombstone of the moved pair in the node, or drop the pair if the buffer is full
            // @end
            parent.set_key_at(idx, &k);
        } else {
            let mut left = Internal::<_, K>::new(&mut left_guard[..]);
            let last = left.size() - 1;
            let (k, child) = left.entry_at(last);
            left.set_size(last);
            let separator = parent.key_at(idx);
            let mut node = Internal::<_, K>::new(&mut node_guard[..]);
            node.insert_at_front(&k, child);
            node.set_key_at(1, &separator); // the old first child now follows `child`, split off by the old separator
            parent.set_key_at(idx, &k);
        }
    }
    //~ // TODO(2c-04): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // @begin 2c-04
    /// Moves the first entry of `right` to the end of `node`; `idx` is `node`'s slot in `parent`, so the separator is at `idx + 1`.
    fn borrow_from_right(&self, parent_guard: &mut WritePageGuard<'a>, idx: u32, node_guard: &mut WritePageGuard<'a>, right_guard: &mut WritePageGuard<'a>, is_leaf: bool) {
        let mut parent = Internal::<_, K>::new(&mut parent_guard[..]);
        if is_leaf {
            let mut right = Leaf::<_, K, V, TOMBS>::new(&mut right_guard[..]);
            let (k, v) = right.entry_at(0);
            // @begin 2d-02
            let was_deleted = right.is_deleted_at(0);
            //~ let was_deleted = false; // TODO(2d-01): a tombstoned pair keeps its tombstone when it moves
            // @end
            right.remove_at(0);
            let mut node = Leaf::<_, K, V, TOMBS>::new(&mut node_guard[..]);
            node.insert(&k, &v, &self.cmp);
            // @begin 2d-02
            if was_deleted {
                if node.tombstones().len() < TOMBS {
                    node.add_tombstone(&k);
                } else {
                    node.remove(&k, &self.cmp);
                }
            }
            //~ // TODO(2d-01): re-buffer the tombstone of the moved pair in the node, or drop the pair if the buffer is full
            // @end
            parent.set_key_at(idx + 1, &right.key_at(0));
        } else {
            let mut right = Internal::<_, K>::new(&mut right_guard[..]);
            let child = right.value_at(0);
            let next_key = right.key_at(1);
            right.remove_at(0);
            let separator = parent.key_at(idx + 1);
            let mut node = Internal::<_, K>::new(&mut node_guard[..]);
            let size = node.size();
            node.set_entry_at(size, &separator, child);
            node.set_size(size + 1);
            parent.set_key_at(idx + 1, &next_key);
        }
    }
    //~ // TODO(2c-04): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // @begin 2c-04
    /// Appends everything in `src` to `dest` (its right neighbour, `src_idx` in the parent). `src` is then empty of meaning: the caller
    /// removes it from the parent and deletes its page.
    fn merge(&self, parent_guard: &mut WritePageGuard<'a>, src_idx: u32, dest_guard: &mut WritePageGuard<'a>, src_guard: &mut WritePageGuard<'a>, is_leaf: bool) {
        if is_leaf {
            let src = Leaf::<_, K, V, TOMBS>::new(&src_guard[..]);
            let mut dest = Leaf::<_, K, V, TOMBS>::new(&mut dest_guard[..]);
            let size = dest.size();
            for i in 0..src.size() {
                let (k, v) = src.entry_at(i);
                dest.set_entry_at(size + i, &k, &v);
            }
            dest.set_size(size + src.size());
            dest.set_next_page_id(src.next_page_id());
            // @begin 2d-02
            let mut tombs = dest.tombstones();
            tombs.extend(src.tombstones());
            // both buffers can be partly full (the short leaf may have been given tombstoned pairs by a sibling): the oldest that do not
            // fit are really removed
            let doomed: Vec<K> = tombs.drain(..tombs.len().saturating_sub(TOMBS)).collect();
            dest.set_tombstones(&tombs);
            for key in &doomed {
                dest.remove(key, &self.cmp);
            }
            //~ // TODO(2d-01): the source's tombstones come along; if the two buffers together are over the limit, really remove the oldest pairs
            // @end
        } else {
            let separator = Internal::<_, K>::new(&parent_guard[..]).key_at(src_idx);
            let src = Internal::<_, K>::new(&src_guard[..]);
            let mut dest = Internal::<_, K>::new(&mut dest_guard[..]);
            let size = dest.size();
            for i in 0..src.size() {
                // the source's first key was never stored; the parent's separator takes its place
                let key = if i == 0 { separator.clone() } else { src.key_at(i) };
                dest.set_entry_at(size + i, &key, src.value_at(i));
            }
            dest.set_size(size + src.size());
        }
    }
    //~ // TODO(2c-04): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // ------------------------------------------------------------------------------------------------------------------------
    // Optimistic paths: assume nothing above the leaf changes
    // ------------------------------------------------------------------------------------------------------------------------

    // @begin 2c-05
    /// Read-latches down to the leaf for `key` and returns it write-latched, with whether it is the root; `None` for an empty tree.
    /// The parent stays read-latched while the leaf is re-latched for writing, so nobody can split or merge the leaf in between.
    fn latch_leaf_for_write(&self, key: &K) -> Option<(WritePageGuard<'a>, bool)> {
        let mut _parent = self.bpm.read_page(self.header_page_id); // held only to keep the latch
        let mut page_id = Header::new(&_parent[..]).root_page_id();
        if !page_id.is_valid() {
            return None;
        }
        let root = page_id;
        loop {
            let guard = self.bpm.read_page(page_id);
            if Page::new(&guard[..]).is_leaf_page() {
                drop(guard);
                let leaf = self.bpm.write_page(page_id); // the parent's read latch keeps this page where it is
                return Some((leaf, page_id == root));
            }
            let child = Internal::<_, K>::new(&guard[..]).child_for(key, &self.cmp);
            _parent = guard; // the old parent guard is dropped here, after the child is latched
            page_id = child;
        }
    }
    //~ // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // @begin 2c-05
    /// `Some(result)` if the insert could be done holding only the leaf's write latch; `None` if it needs the pessimistic path.
    fn insert_optimistic(&self, key: &K, value: &V) -> Option<bool> {
        let (mut leaf_guard, _) = self.latch_leaf_for_write(key)?;
        let mut leaf = Leaf::<_, K, V, TOMBS>::new(&mut leaf_guard[..]);
        if leaf.find(key, &self.cmp).is_some() {
            return Some(leaf.insert(key, value, &self.cmp)); // a live duplicate: false; a tombstoned pair comes back to life (no new slot)
        }
        if leaf.size() + 1 < leaf.max_size() {
            return Some(leaf.insert(key, value, &self.cmp));
        }
        None
    }
    //~ // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // @begin 2c-05
    /// True if the remove was done (or there was nothing to do) holding only the leaf's write latch.
    fn remove_optimistic(&self, key: &K) -> bool {
        let Some((mut leaf_guard, is_root)) = self.latch_leaf_for_write(key) else { return true };
        let mut leaf = Leaf::<_, K, V, TOMBS>::new(&mut leaf_guard[..]);
        if leaf.lookup(key, &self.cmp).is_none() {
            return true;
        }
        // @begin 2d-01
        if TOMBS > 0 {
            // a logical delete removes a pair for real only when the buffer is full, and never changes the size otherwise
            let loses_a_pair = leaf.num_tombstones() as usize >= TOMBS;
            if is_root || !loses_a_pair || leaf.size() > leaf.min_size() {
                return leaf.remove_logically(key, &self.cmp);
            }
            return false;
        }
        //~ // TODO(2d-01): with tombstones the optimistic remove is a logical delete
        // @end
        if (is_root && leaf.size() > 1) || (!is_root && leaf.size() > leaf.min_size()) {
            return leaf.remove(key, &self.cmp);
        }
        false
    }
    //~ // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.
    // @end

    // ------------------------------------------------------------------------------------------------------------------------
    // Scans
    // ------------------------------------------------------------------------------------------------------------------------

    /// An iterator at the smallest key.
    pub fn begin(&self) -> IndexIterator<'a, K, V, TOMBS> {
        // @begin 2c-03
        match self.find_leaf(None) {
            Some(leaf_guard) => {
                // let go of the latch first: the iterator latches the leaf again, and a second read latch on a page this thread already
                // holds can wait behind a writer that is waiting for the first one
                let leaf = leaf_guard.get_page_id();
                drop(leaf_guard);
                IndexIterator::at(self.bpm.inner(), leaf, 0)
            }
            None => self.end(),
        }
        //~ todo!("2c-01: the leftmost leaf, slot 0; an empty tree has no leaf, so its begin is its end")
        // @end
    }

    /// An iterator at the first key that is not less than `key`.
    pub fn begin_at(&self, key: &K) -> IndexIterator<'a, K, V, TOMBS> {
        // @begin 2c-03
        match self.find_leaf(Some(key)) {
            Some(leaf_guard) => {
                let at = Leaf::<_, K, V, TOMBS>::new(&leaf_guard[..]).lower_bound(key, &self.cmp);
                let leaf = leaf_guard.get_page_id();
                drop(leaf_guard); // see `begin`
                IndexIterator::at(self.bpm.inner(), leaf, at)
            }
            None => self.end(),
        }
        //~ todo!("2c-01: the leaf for the key, at the first slot whose key is not less than it (IndexIterator::at moves on if that is past the leaf's end)")
        // @end
    }

    /// The iterator past the last pair.
    pub fn end(&self) -> IndexIterator<'a, K, V, TOMBS> {
        // @begin 2c-03
        IndexIterator::end(self.bpm.inner())
        //~ todo!("2c-01: IndexIterator::end")
        // @end
    }
}
