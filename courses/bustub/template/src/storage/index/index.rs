//! What the rest of the engine sees of an index. An `Index` maps a **key tuple** (the indexed columns of a row) to the [`Rid`] of the row. The B+
//! tree stores fixed-size byte keys ([`GenericKey`]), so this layer converts: a key tuple becomes the bytes of a `GenericKey<N>`, and a comparator
//! that knows the key's schema orders those bytes by their column values, left to right.
//!
//! Only INTEGER columns are indexed, at 4 bytes per column.

use std::cmp::Ordering;
use std::sync::Arc;

use super::b_plus_tree::BPlusTree;
use super::fixed_size::FixedSize;
use super::generic_key::{GenericKey, KeyComparator};
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::catalog::schema::Schema;
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

pub trait Index: Send + Sync {
    fn metadata(&self) -> &IndexMetadata;

    /// Adds `key -> rid`. `false` if the key is already there: keys are unique.
    fn insert_entry(&self, key: &Tuple, rid: Rid) -> bool;

    /// Removes the entry for `key`, if there is one.
    fn delete_entry(&self, key: &Tuple);

    /// The rids stored under `key`: none or one.
    fn scan_key(&self, key: &Tuple) -> Vec<Rid>;

    /// Every rid in key order.
    fn scan_all(&self) -> Vec<Rid>;

    /// The rids of the keys that are not less than `key`, in key order.
    fn scan_from(&self, key: &Tuple) -> Vec<Rid>;
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

/// Orders keys by their columns, left to right, comparing values.
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

/// An index backed by a B+ tree with `N`-byte keys.
pub struct BPlusTreeIndex<'a, const N: usize> {
    metadata: IndexMetadata,
    tree: BPlusTree<'a, GenericKey<N>, Rid, SchemaComparator<N>>,
}

impl<'a, const N: usize> BPlusTreeIndex<'a, N> {
    /// Allocates the tree's header page and builds an empty tree with the default node sizes.
    pub fn new(metadata: IndexMetadata, bpm: &'a BufferPoolManager) -> BPlusTreeIndex<'a, N> {
        todo!("3c-03: allocate a page for the tree's header; a SchemaComparator over the key schema; a B+ tree with the default sizes")
    }
}

impl<const N: usize> BPlusTreeIndex<'_, N> {
    // TODO(3c-03): a private helper of yours, if you want one
}

impl<const N: usize> Index for BPlusTreeIndex<'_, N> {
    fn metadata(&self) -> &IndexMetadata {
        &self.metadata
    }

    fn insert_entry(&self, key: &Tuple, rid: Rid) -> bool {
        todo!("3c-03: a key with a NULL in it is not indexed (false); otherwise the key tuple as a GenericKey, inserted into the tree")
    }

    fn delete_entry(&self, key: &Tuple) {
        todo!("3c-03: remove the key from the tree (a key with a NULL was never there)")
    }

    fn scan_key(&self, key: &Tuple) -> Vec<Rid> {
        todo!("3c-03: the tree's lookup of the key; a key with a NULL in it finds nothing")
    }

    fn scan_all(&self) -> Vec<Rid> {
        todo!("3c-03: the rids of the tree's iterator, in order")
    }

    fn scan_from(&self, key: &Tuple) -> Vec<Rid> {
        todo!("3c-03: the rids from the tree's begin_at(key), in order")
    }
}

#[allow(dead_code)]
fn _key_size_is_fixed<const N: usize>() -> usize {
    <GenericKey<N> as FixedSize>::SIZE
}
