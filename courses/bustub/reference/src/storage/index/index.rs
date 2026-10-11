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
        // @begin 3c-03
        for i in 0..self.key_schema.column_count() {
            let (l, r) = (key_to_value(lhs, &self.key_schema, i), key_to_value(rhs, &self.key_schema, i));
            // a NULL is smaller than every value and equal to another NULL (the index never stores one, but the order must be total)
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
        //~ todo!("3c-03: compare column by column (key_to_value for each side): a NULL is smaller than any value and equal to a NULL; otherwise the first column that differs decides; all equal is Equal")
        // @end
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
        // @begin 3c-03
        let header_page_id = bpm.new_page();
        let comparator = SchemaComparator::<N>::new(metadata.get_key_schema().clone());
        let tree = BPlusTree::new(
            metadata.get_name(),
            header_page_id,
            bpm,
            comparator,
            BPlusTree::<GenericKey<N>, Rid, SchemaComparator<N>>::default_leaf_max_size(),
            BPlusTree::<GenericKey<N>, Rid, SchemaComparator<N>>::default_internal_max_size(),
        );
        BPlusTreeIndex { metadata, tree }
        //~ todo!("3c-03: allocate a page for the tree's header; a SchemaComparator over the key schema; a B+ tree with the default sizes")
        // @end
    }
}

impl<const N: usize> BPlusTreeIndex<'_, N> {
    // @begin 3c-03
    /// True if any column of the key tuple is NULL.
    fn has_null(&self, key: &Tuple) -> bool {
        let schema = self.metadata.get_key_schema();
        (0..schema.column_count()).any(|i| key.get_value(schema, i).is_null())
    }
    //~ // TODO(3c-03): a private helper of yours, if you want one
    // @end
}

impl<const N: usize> Index for BPlusTreeIndex<'_, N> {
    fn metadata(&self) -> &IndexMetadata {
        &self.metadata
    }

    fn insert_entry(&self, key: &Tuple, rid: Rid) -> bool {
        // @begin 3c-03
        if self.has_null(key) {
            return false; // a NULL is not equal to anything, so a row with a NULL key can never be looked up by key: it is not indexed
        }
        self.tree.insert(&generic_key_from_tuple::<N>(key), &rid)
        //~ todo!("3c-03: a key with a NULL in it is not indexed (false); otherwise the key tuple as a GenericKey, inserted into the tree")
        // @end
    }

    fn delete_entry(&self, key: &Tuple) {
        // @begin 3c-03
        if !self.has_null(key) {
            self.tree.remove(&generic_key_from_tuple::<N>(key));
        }
        //~ todo!("3c-03: remove the key from the tree (a key with a NULL was never there)")
        // @end
    }

    fn scan_key(&self, key: &Tuple) -> Vec<Rid> {
        // @begin 3c-03
        if self.has_null(key) {
            return Vec::new(); // NULL = anything is never true
        }
        self.tree.get_value(&generic_key_from_tuple::<N>(key))
        //~ todo!("3c-03: the tree's lookup of the key; a key with a NULL in it finds nothing")
        // @end
    }

    fn scan_all(&self) -> Vec<Rid> {
        // @begin 3c-03
        self.tree.begin().map(|(_, rid)| rid).collect()
        //~ todo!("3c-03: the rids of the tree's iterator, in order")
        // @end
    }

    fn scan_from(&self, key: &Tuple) -> Vec<Rid> {
        // @begin 3c-03
        self.tree.begin_at(&generic_key_from_tuple::<N>(key)).map(|(_, rid)| rid).collect()
        //~ todo!("3c-03: the rids from the tree's begin_at(key), in order")
        // @end
    }
}

#[allow(dead_code)]
fn _key_size_is_fixed<const N: usize>() -> usize {
    <GenericKey<N> as FixedSize>::SIZE
}
