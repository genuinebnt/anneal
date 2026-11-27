//! What the rest of the engine sees of an index. An `Index` holds **entries** `(key, rid)`: the indexed columns of a row (a **key
//! tuple**) and the [`Rid`] of the row. Entries are ordered by key, then by rid, so the entries of one key sit together, in rid order.
//!
//! An index that is not a primary key holds one entry per row: two rows may have the same key, and a key with a NULL in it is stored
//! like any other (a NULL orders before every value). A primary-key index holds at most one entry per key and none with a NULL.
//!
//! The B+ tree stores fixed-size byte keys ([`GenericKey`]), so this layer converts: an entry becomes the bytes of its key tuple followed
//! by the 8 bytes of its rid, and a comparator that knows the key's schema orders those bytes by column values, left to right, then by
//! rid.
//!
//! Only INTEGER columns are indexed, at 4 bytes per column.

use std::cmp::Ordering;
use std::sync::Arc;

use super::b_plus_tree::BPlusTree;
use super::fixed_size::FixedSize;
use super::generic_key::{GenericKey, KeyComparator};
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::catalog::schema::Schema;
use crate::common::config::PageId;
use crate::common::exception::{Exception, ExceptionType};
use crate::common::rid::Rid;
use crate::storage::table::tuple::Tuple;
use crate::types::value::Value;

/// The kind of index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexType {
    BPlusTreeIndex,
}

/// What an index is made of: its name, its table, which columns of the table it covers, and the schema of its keys.
#[derive(Clone, Debug)]
pub struct IndexMetadata {
    name: String,
    table_name: String,
    /// The columns of the table that form the key, in key order.
    key_attrs: Vec<u32>,
    key_schema: Arc<Schema>,
    is_primary_key: bool,
}

impl IndexMetadata {
    /// The key schema is the table's columns `key_attrs`, as `Schema::copy_schema` makes it.
    pub fn new(name: &str, table_name: &str, tuple_schema: &Schema, key_attrs: Vec<u32>, is_primary_key: bool) -> IndexMetadata {
        // @begin 3c-03
        let key_schema = Arc::new(Schema::copy_schema(tuple_schema, &key_attrs));
        IndexMetadata { name: name.to_owned(), table_name: table_name.to_owned(), key_attrs, key_schema, is_primary_key }
        //~ todo!("3c-03: remember the names and the key columns; the key schema is the table schema restricted to them (copy_schema)")
        // @end
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_table_name(&self) -> &str {
        &self.table_name
    }

    pub fn get_key_schema(&self) -> &Arc<Schema> {
        &self.key_schema
    }

    pub fn get_key_attrs(&self) -> &[u32] {
        &self.key_attrs
    }

    pub fn is_primary_key(&self) -> bool {
        self.is_primary_key
    }

    /// How many columns the key has.
    pub fn get_index_column_count(&self) -> u32 {
        self.key_attrs.len() as u32
    }
}

/// True if any column of `key` (laid out with `key_schema`) is NULL. Such a key equals no key, itself included.
pub fn key_has_null(key: &Tuple, key_schema: &Schema) -> bool {
    (0..key_schema.column_count()).any(|i| key.get_value(key_schema, i).is_null())
}

/// The error of a statement that would give a second live row a key the unique index `index_name` already holds. It is an `Invalid`
/// error, so it reaches the caller with its message instead of only failing the statement, and its SQLSTATE is `23505`.
pub fn unique_violation(index_name: &str) -> Exception {
    Exception::new(ExceptionType::Invalid, format!("duplicate key value violates unique constraint \"{index_name}\""))
        .with_sqlstate("23505")
}

/// The error of a statement that would store a NULL in column `column` of the primary key of `table`. Its SQLSTATE is `23502`.
pub fn not_null_violation(column: &str, table: &str) -> Exception {
    Exception::new(
        ExceptionType::Invalid,
        format!("null value in column \"{column}\" of relation \"{table}\" violates not-null constraint"),
    )
    .with_sqlstate("23502")
}

/// An ordered set of entries `(key, rid)`, ordered by key, then by rid.
///
/// An index that is not a primary key takes any entry: many rows may share a key, and a key with a NULL in it is stored too. A
/// primary-key index (`metadata().is_primary_key()`) is **unique**: it holds at most one entry per key and none whose key has a NULL.
pub trait Index: Send + Sync {
    fn metadata(&self) -> &IndexMetadata;

    /// Adds the entry `(key, rid)`. `false`, and nothing is added, if the entry is already there, or if the index is unique and `key`
    /// is already there (with any rid) or has a NULL in it.
    fn insert_entry(&self, key: &Tuple, rid: Rid) -> bool;

    /// Removes the entry `(key, rid)`, if there is one. The other entries of `key` stay.
    fn delete_entry(&self, key: &Tuple, rid: Rid);

    /// The rids of the entries whose key equals `key`, in rid order. None if `key` has a NULL in it: NULL equals nothing.
    fn scan_key(&self, key: &Tuple) -> Vec<Rid>;

    /// Every entry, in order.
    fn scan_all(&self) -> Vec<(Tuple, Rid)>;

    /// The entries whose key is not less than `key`, in order.
    fn scan_from(&self, key: &Tuple) -> Vec<(Tuple, Rid)>;
}

/// The bytes of `tuple` as an index key of `N` bytes (zero padded). A key tuple that is longer than `N` is a bug.
pub fn generic_key_from_tuple<const N: usize>(tuple: &Tuple) -> GenericKey<N> {
    // @begin 3c-03
    assert!(tuple.data().len() <= N, "a key of {} bytes does not fit an index key of {N}", tuple.data().len());
    let mut key = GenericKey::<N>::default();
    key.data[..tuple.data().len()].copy_from_slice(tuple.data());
    key
    //~ todo!("3c-03: a zeroed key with the tuple's bytes at the start (panic if the tuple is longer than N)")
    // @end
}

/// Reads column `column_idx` of a key as a value, using the key's schema.
pub fn key_to_value<const N: usize>(key: &GenericKey<N>, schema: &Schema, column_idx: u32) -> Value {
    let column = schema.column(column_idx);
    Value::deserialize_from(&key.data[column.offset() as usize..], column.type_id()).expect("a key column has a valid type")
}

/// Orders keys by their columns, left to right, comparing values. A NULL is smaller than every value and equal to another NULL, so
/// the order is total.
#[derive(Clone)]
pub struct SchemaComparator<const N: usize> {
    key_schema: Arc<Schema>,
}

impl<const N: usize> SchemaComparator<N> {
    pub fn new(key_schema: Arc<Schema>) -> SchemaComparator<N> {
        SchemaComparator { key_schema }
    }
}

impl<const N: usize> KeyComparator<GenericKey<N>> for SchemaComparator<N> {
    fn compare(&self, lhs: &GenericKey<N>, rhs: &GenericKey<N>) -> Ordering {
        // @begin 3c-03
        for i in 0..self.key_schema.column_count() {
            let (l, r) = (key_to_value(lhs, &self.key_schema, i), key_to_value(rhs, &self.key_schema, i));
            match (l.is_null(), r.is_null()) {
                (true, true) => continue,
                (true, false) => return Ordering::Less,
                (false, true) => return Ordering::Greater,
                (false, false) => {}
            }
            if l.compare_less_than(&r).unwrap() == crate::types::value::CmpBool::True {
                return Ordering::Less;
            }
            if l.compare_greater_than(&r).unwrap() == crate::types::value::CmpBool::True {
                return Ordering::Greater;
            }
        }
        Ordering::Equal
        //~ todo!("3c-03: column by column: a NULL is smaller than any value and equal to a NULL; the first column that differs decides; all equal is Equal")
        // @end
    }
}

/// Orders the keys of index entries, each the bytes of a key tuple followed by the 8 bytes of a rid: by the key columns
/// ([`SchemaComparator`]), then by the rid. In a unique index the rid takes no part, so two entries with one key are equal and the
/// tree holds the key once.
#[derive(Clone)]
pub struct EntryComparator<const N: usize> {
    columns: SchemaComparator<N>,
    /// Where the rid starts (the size of the key columns), or `None` in a unique index.
    rid_offset: Option<usize>,
}

impl<const N: usize> EntryComparator<N> {
    /// Orders entries whose key tuples are laid out with `key_schema`; `unique` leaves the rid out of the order.
    pub fn new(key_schema: Arc<Schema>, unique: bool) -> EntryComparator<N> {
        // @begin 3c-03
        let rid_offset = (!unique).then(|| key_schema.inlined_storage_size() as usize);
        EntryComparator { columns: SchemaComparator::new(key_schema), rid_offset }
        //~ todo!("3c-03: the column order over key_schema; the rid starts right after the key columns, and a unique index ignores it")
        // @end
    }
}

impl<const N: usize> KeyComparator<GenericKey<N>> for EntryComparator<N> {
    fn compare(&self, lhs: &GenericKey<N>, rhs: &GenericKey<N>) -> Ordering {
        // @begin 3c-03
        let by_key = self.columns.compare(lhs, rhs);
        match self.rid_offset {
            Some(at) if by_key == Ordering::Equal => rid_at(lhs, at).cmp(&rid_at(rhs, at)),
            _ => by_key,
        }
        //~ todo!("3c-03: by the key columns; on a tie, by the rid stored after them, unless the index is unique")
        // @end
    }
}

// @begin 3c-03
/// The rid stored in `key` at byte `at`: the page id, then the slot, both little-endian.
fn rid_at<const N: usize>(key: &GenericKey<N>, at: usize) -> Rid {
    let page_id = i32::from_le_bytes(key.data[at..at + 4].try_into().unwrap());
    let slot = u32::from_le_bytes(key.data[at + 4..at + 8].try_into().unwrap());
    Rid::new(PageId(page_id), slot)
}

/// The smallest rid there can be: an entry with it comes before every entry of its key.
fn smallest_rid() -> Rid {
    Rid::new(PageId(i32::MIN), 0)
}
//~ // TODO(3c-03): private helpers of yours, if you want some
// @end

/// An index backed by a B+ tree with `N`-byte keys. Each tree key is an entry: the key tuple's bytes, then the rid's 8 bytes, so `N`
/// is at least the key's size plus 8.
pub struct BPlusTreeIndex<'a, const N: usize> {
    metadata: IndexMetadata,
    tree: BPlusTree<'a, GenericKey<N>, Rid, EntryComparator<N>>,
}

impl<'a, const N: usize> BPlusTreeIndex<'a, N> {
    /// Allocates the tree's header page and builds an empty tree with the default node sizes. Panics if a key tuple and a rid do not
    /// fit in `N` bytes.
    pub fn new(metadata: IndexMetadata, bpm: &'a BufferPoolManager) -> BPlusTreeIndex<'a, N> {
        // @begin 3c-03
        let key_size = metadata.get_key_schema().inlined_storage_size() as usize;
        assert!(key_size + 8 <= N, "an entry of a {key_size}-byte key and a rid does not fit an index key of {N} bytes");
        let header_page_id = bpm.new_page();
        let comparator = EntryComparator::<N>::new(metadata.get_key_schema().clone(), metadata.is_primary_key());
        let tree = BPlusTree::new(
            metadata.get_name(),
            header_page_id,
            bpm,
            comparator,
            BPlusTree::<GenericKey<N>, Rid, EntryComparator<N>>::default_leaf_max_size(),
            BPlusTree::<GenericKey<N>, Rid, EntryComparator<N>>::default_internal_max_size(),
        );
        BPlusTreeIndex { metadata, tree }
        //~ todo!("3c-03: panic unless the key and a rid fit in N bytes; allocate a page for the tree's header; an EntryComparator over the key schema (unique for a primary key); a B+ tree with the default sizes")
        // @end
    }
}

impl<const N: usize> BPlusTreeIndex<'_, N> {
    // @begin 3c-03
    /// The size of a key tuple in bytes.
    fn key_size(&self) -> usize {
        self.metadata.get_key_schema().inlined_storage_size() as usize
    }

    /// The tree key of the entry `(key, rid)`: the key's bytes, then the rid's (a unique index orders by the key alone, so the rid
    /// bytes it stores do not matter).
    fn entry(&self, key: &Tuple, rid: Rid) -> GenericKey<N> {
        let mut entry = generic_key_from_tuple::<N>(key);
        let at = self.key_size();
        entry.data[at..at + 4].copy_from_slice(&rid.page_id().0.to_le_bytes());
        entry.data[at + 4..at + 8].copy_from_slice(&rid.slot_num().to_le_bytes());
        entry
    }

    /// The key tuple an entry starts with.
    fn key_of(&self, entry: &GenericKey<N>) -> Tuple {
        Tuple::from_bytes(Rid::default(), &entry.data[..self.key_size()])
    }

    /// The entries from the first one not less than `(key, rid)`, in order, while `more` accepts them.
    fn entries_from(&self, key: &Tuple, rid: Rid, more: impl Fn(&Tuple) -> bool) -> Vec<(Tuple, Rid)> {
        self.tree.begin_at(&self.entry(key, rid)).map(|(entry, rid)| (self.key_of(&entry), rid)).take_while(|(k, _)| more(k)).collect()
    }
    //~ // TODO(3c-03): private helpers of yours, if you want some
    // @end
}

impl<const N: usize> Index for BPlusTreeIndex<'_, N> {
    fn metadata(&self) -> &IndexMetadata {
        &self.metadata
    }

    fn insert_entry(&self, key: &Tuple, rid: Rid) -> bool {
        // @begin 3c-03
        if self.metadata.is_primary_key() && key_has_null(key, self.metadata.get_key_schema()) {
            return false; // a NULL identifies no row: a primary key never holds one
        }
        self.tree.insert(&self.entry(key, rid), &rid)
        //~ todo!("3c-03: the entry (key, rid) into the tree; a unique index refuses a key it holds and a key with a NULL")
        // @end
    }

    fn delete_entry(&self, key: &Tuple, rid: Rid) {
        // @begin 3c-03
        let entry = self.entry(key, rid);
        // in a unique index the tree finds the key's entry whatever its rid: remove it only if it is this row's
        if !self.metadata.is_primary_key() || self.tree.get_value(&entry) == [rid] {
            self.tree.remove(&entry);
        }
        //~ todo!("3c-03: remove the entry (key, rid), and only that one: in a unique index the key's entry may belong to another rid")
        // @end
    }

    fn scan_key(&self, key: &Tuple) -> Vec<Rid> {
        // @begin 3c-03
        if key_has_null(key, self.metadata.get_key_schema()) {
            return Vec::new(); // NULL = anything is never true
        }
        self.entries_from(key, smallest_rid(), |k| k.data() == key.data()).into_iter().map(|(_, rid)| rid).collect()
        //~ todo!("3c-03: the rids of the key's entries, in rid order (they sit together in the tree); a key with a NULL finds nothing")
        // @end
    }

    fn scan_all(&self) -> Vec<(Tuple, Rid)> {
        // @begin 3c-03
        self.tree.begin().map(|(entry, rid)| (self.key_of(&entry), rid)).collect()
        //~ todo!("3c-03: every entry of the tree's iterator, as (key tuple, rid), in order")
        // @end
    }

    fn scan_from(&self, key: &Tuple) -> Vec<(Tuple, Rid)> {
        // @begin 3c-03
        self.entries_from(key, smallest_rid(), |_| true)
        //~ todo!("3c-03: the entries from the first whose key is not less than key, in order")
        // @end
    }
}

#[allow(dead_code)]
fn _key_size_is_fixed<const N: usize>() -> usize {
    <GenericKey<N> as FixedSize>::SIZE
}
