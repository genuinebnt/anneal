//! The database's catalog: which tables and indexes exist and where they live. It is held in memory for the life of the instance.
//!
//! Tables and indexes have a name (unique among tables; unique per table for indexes) and an **oid**, handed out in creation order.

use std::collections::HashMap;
use std::sync::Arc;

use super::schema::Schema;
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::storage::index::generic_key::GenericKey;
use crate::storage::index::index::{key_has_null, not_null_violation, unique_violation, BPlusTreeIndex, Index, IndexMetadata, IndexType};
use crate::storage::table::tuple::Tuple;
use crate::storage::table::table_heap::TableHeap;

pub type TableOid = u32;
pub type ColumnOid = u32;
pub type IndexOid = u32;

/// What the catalog knows of a table: its schema, name, oid and heap.
pub struct TableInfo<'a> {
    pub schema: Schema,
    pub name: String,
    pub table: TableHeap<'a>,
    pub oid: TableOid,
}

/// What the catalog knows of an index.
pub struct IndexInfo<'a> {
    pub key_schema: Schema,
    pub name: String,
    pub index: Box<dyn Index + 'a>,
    pub index_oid: IndexOid,
    pub table_name: String,
    /// The size of the key in bytes: 4 per integer column.
    pub key_size: usize,
    pub is_primary_key: bool,
    pub index_type: IndexType,
}

impl IndexInfo<'_> {
    /// Fails with a not-null violation if this is a primary-key index and `key` (laid out with the key schema) has a NULL: a primary
    /// key identifies its row, and a NULL identifies nothing. Any key passes an index that is not a primary key.
    pub fn check_primary_key(&self, key: &Tuple) -> Result<()> {
        if !self.is_primary_key {
            return Ok(());
        }
        match (0..self.key_schema.column_count()).find(|i| key.get_value(&self.key_schema, *i).is_null()) {
            Some(i) => Err(not_null_violation(self.key_schema.column(i).name(), &self.table_name)),
            None => Ok(()),
        }
    }
}

pub struct Catalog<'a> {
    _catalog: std::marker::PhantomData<&'a BufferPoolManager>,
    // TODO(3c-04): the fields are yours: the pool, the tables and indexes by name and by oid, and the next oids.
}

impl<'a> Catalog<'a> {
    /// An empty catalog over `bpm`.
    pub fn new(bpm: &'a BufferPoolManager) -> Catalog<'a> {
        todo!("3c-04: an empty catalog over the buffer pool")
    }

    /// Creates a table: a new heap, the next table oid. `None` if a table of that name exists.
    pub fn create_table(&mut self, table_name: &str, schema: &Schema) -> Option<Arc<TableInfo<'a>>> {
        todo!("3c-04: None if the name is taken; otherwise a table with a new heap and the next oid, found by oid and by name")
    }

    /// The table named `table_name`, if there is one.
    pub fn get_table(&self, table_name: &str) -> Option<Arc<TableInfo<'a>>> {
        todo!("3c-04: look the name up, then the oid")
    }

    /// The table with this `oid`, if there is one.
    pub fn get_table_by_oid(&self, oid: TableOid) -> Option<Arc<TableInfo<'a>>> {
        todo!("3c-04: look the oid up")
    }

    /// The table with this `oid`, borrowed from the catalog: the reference lives as long as the catalog and costs no `Arc` clone.
    pub fn table_info(&self, oid: TableOid) -> Option<&TableInfo<'a>> {
        todo!("3c-04: the table with that oid, borrowed from the catalog")
    }

    /// The names of all tables (in no particular order).
    pub fn get_table_names(&self) -> Vec<String> {
        todo!("3c-04: every table's name")
    }

    /// Creates an index on the columns `key_attrs` of `table_name`, fills it with the table's live tuples and returns it. `Ok(None)` if
    /// the table does not exist or the table already has an index of that name.
    ///
    /// Every live tuple whose key has no NULL gets exactly one entry; a tuple whose key has a NULL gets none.
    ///
    /// # Errors
    ///
    /// The key is not made of INTEGER columns or is bigger than 64 bytes, or two live tuples have the same non-NULL key (a unique
    /// violation). No index is created then.
    pub fn create_index(&mut self, index_name: &str, table_name: &str, key_attrs: Vec<u32>, is_primary_key: bool) -> Result<Option<Arc<IndexInfo<'a>>>> {
        todo!("3c-04: Ok(None) for a missing table or a taken name; an error unless the key is 1 to 64 bytes of INTEGER columns, or if two live tuples share a non-NULL key; the index holds every live tuple whose key has no NULL")
    }

    /// The index `index_name` on `table_name`, if there is one.
    pub fn get_index(&self, index_name: &str, table_name: &str) -> Option<Arc<IndexInfo<'a>>> {
        todo!("3c-04: the table's index of that name, if any")
    }

    /// The index with this `index_oid`, if there is one.
    pub fn get_index_by_oid(&self, index_oid: IndexOid) -> Option<Arc<IndexInfo<'a>>> {
        todo!("3c-04: look the oid up")
    }

    /// Every index of the table (none for an unknown table), in creation order.
    pub fn get_table_indexes(&self, table_name: &str) -> Vec<Arc<IndexInfo<'a>>> {
        todo!("3c-04: the table's indexes, ordered by oid")
    }
}

#[allow(dead_code)]
type _Unused = GenericKey<4>;
