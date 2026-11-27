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
        todo!("3c-03: remember the names and the key columns; the key schema is the table schema restricted to them (copy_schema)")
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
    todo!("3c-03: a zeroed key with the tuple's bytes at the start (panic if the tuple is longer than N)")
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
        todo!("3c-03: column by column: a NULL is smaller than any value and equal to a NULL; the first column that differs decides; all equal is Equal")
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
        todo!("3c-03: the column order over key_schema; the rid starts right after the key columns, and a unique index ignores it")
    }
}

impl<const N: usize> KeyComparator<GenericKey<N>> for EntryComparator<N> {
    fn compare(&self, lhs: &GenericKey<N>, rhs: &GenericKey<N>) -> Ordering {
        todo!("3c-03: by the key columns; on a tie, by the rid stored after them, unless the index is unique")
    }
}

// TODO(3c-03): private helpers of yours, if you want some

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
        todo!("3c-03: panic unless the key and a rid fit in N bytes; allocate a page for the tree's header; an EntryComparator over the key schema (unique for a primary key); a B+ tree with the default sizes")
    }
}

impl<const N: usize> BPlusTreeIndex<'_, N> {
    // TODO(3c-03): private helpers of yours, if you want some
}

impl<const N: usize> Index for BPlusTreeIndex<'_, N> {
    fn metadata(&self) -> &IndexMetadata {
        &self.metadata
    }

    fn insert_entry(&self, key: &Tuple, rid: Rid) -> bool {
        todo!("3c-03: the entry (key, rid) into the tree; a unique index refuses a key it holds and a key with a NULL")
    }

    fn delete_entry(&self, key: &Tuple, rid: Rid) {
        todo!("3c-03: remove the entry (key, rid), and only that one: in a unique index the key's entry may belong to another rid")
    }

    fn scan_key(&self, key: &Tuple) -> Vec<Rid> {
        todo!("3c-03: the rids of the key's entries, in rid order (they sit together in the tree); a key with a NULL finds nothing")
    }

    fn scan_all(&self) -> Vec<(Tuple, Rid)> {
        todo!("3c-03: every entry of the tree's iterator, as (key tuple, rid), in order")
    }

    fn scan_from(&self, key: &Tuple) -> Vec<(Tuple, Rid)> {
        todo!("3c-03: the entries from the first whose key is not less than key, in order")
    }
}

#[allow(dead_code)]
fn _key_size_is_fixed<const N: usize>() -> usize {
    <GenericKey<N> as FixedSize>::SIZE
}
