//! The database's catalog: which tables and indexes exist and where they live. It is held in memory for the life of the instance.
//!
//! Tables and indexes have a name (unique among tables; unique per table for indexes) and an **oid**, handed out in creation order.

use std::collections::HashMap;
use std::sync::Arc;

use super::schema::Schema;
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::storage::index::generic_key::GenericKey;
use crate::storage::index::index::{BPlusTreeIndex, Index, IndexMetadata, IndexType};
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

pub struct Catalog<'a> {
    // @begin 3c-04
    bpm: &'a BufferPoolManager,
    tables: HashMap<TableOid, Arc<TableInfo<'a>>>,
    table_names: HashMap<String, TableOid>,
    indexes: HashMap<IndexOid, Arc<IndexInfo<'a>>>,
    /// table name -> (index name -> oid)
    index_names: HashMap<String, HashMap<String, IndexOid>>,
    next_table_oid: TableOid,
    next_index_oid: IndexOid,
    //~ _catalog: std::marker::PhantomData<&'a BufferPoolManager>,
    //~ // TODO(3c-04): the fields are yours: the pool, the tables and indexes by name and by oid, and the next oids.
    // @end
}

impl<'a> Catalog<'a> {
    /// An empty catalog over `bpm`.
    pub fn new(bpm: &'a BufferPoolManager) -> Catalog<'a> {
        // @begin 3c-04
        Catalog { bpm, tables: HashMap::new(), table_names: HashMap::new(), indexes: HashMap::new(), index_names: HashMap::new(), next_table_oid: 0, next_index_oid: 0 }
        //~ todo!("3c-04: an empty catalog over the buffer pool")
        // @end
    }

    /// Creates a table: a new heap, the next table oid. `None` if a table of that name exists.
    pub fn create_table(&mut self, table_name: &str, schema: &Schema) -> Option<Arc<TableInfo<'a>>> {
        // @begin 3c-04
        if self.table_names.contains_key(table_name) {
            return None;
        }
        let oid = self.next_table_oid;
        self.next_table_oid += 1;
        let info = Arc::new(TableInfo { schema: schema.clone(), name: table_name.to_owned(), table: TableHeap::new(self.bpm), oid });
        self.tables.insert(oid, info.clone());
        self.table_names.insert(table_name.to_owned(), oid);
        self.index_names.insert(table_name.to_owned(), HashMap::new());
        Some(info)
        //~ todo!("3c-04: None if the name is taken; otherwise a table with a new heap and the next oid, found by oid and by name")
        // @end
    }

    /// The table named `table_name`, if there is one.
    pub fn get_table(&self, table_name: &str) -> Option<Arc<TableInfo<'a>>> {
        // @begin 3c-04
        self.table_names.get(table_name).and_then(|oid| self.tables.get(oid)).cloned()
        //~ todo!("3c-04: look the name up, then the oid")
        // @end
    }

    /// The table with this `oid`, if there is one.
    pub fn get_table_by_oid(&self, oid: TableOid) -> Option<Arc<TableInfo<'a>>> {
        // @begin 3c-04
        self.tables.get(&oid).cloned()
        //~ todo!("3c-04: look the oid up")
        // @end
    }

    /// The table with this `oid`, borrowed from the catalog: the reference lives as long as the catalog and costs no `Arc` clone.
    pub fn table_info(&self, oid: TableOid) -> Option<&TableInfo<'a>> {
        // @begin 3c-04
        self.tables.get(&oid).map(|t| t.as_ref())
        //~ todo!("3c-04: the table with that oid, borrowed from the catalog")
        // @end
    }

    /// The names of all tables (in no particular order).
    pub fn get_table_names(&self) -> Vec<String> {
        // @begin 3c-04
        self.table_names.keys().cloned().collect()
        //~ todo!("3c-04: every table's name")
        // @end
    }

    /// Creates an index on the columns `key_attrs` of `table_name`, fills it with the table's existing tuples and returns it. `Ok(None)` if
    /// the table does not exist or the table already has an index of that name; an error if the key is not made of INTEGER columns or is
    /// bigger than 64 bytes. Tuples whose key is already in the index are ignored (the tree keeps unique keys).
    pub fn create_index(&mut self, index_name: &str, table_name: &str, key_attrs: Vec<u32>, is_primary_key: bool) -> Result<Option<Arc<IndexInfo<'a>>>> {
        // @begin 3c-04
        let Some(table) = self.get_table(table_name) else { return Ok(None) };
        if self.index_names[table_name].contains_key(index_name) {
            return Ok(None);
        }
        let metadata = IndexMetadata::new(index_name, table_name, &table.schema, key_attrs.clone(), is_primary_key);
        let key_schema = metadata.get_key_schema().clone();
        for column in key_schema.columns() {
            if column.type_id() != crate::types::type_id::TypeId::Integer {
                return Err(Exception::new(ExceptionType::NotImplemented, "only support creating index on integer column"));
            }
        }
        let key_size = key_attrs.len() * 4;
        // the smallest key size the tree supports that fits
        let index: Box<dyn Index + 'a> = match key_size {
            0 => return Err(Exception::new(ExceptionType::NotImplemented, "Index columns cannot be empty")),
            1..=4 => Box::new(BPlusTreeIndex::<4>::new(metadata, self.bpm)),
            5..=8 => Box::new(BPlusTreeIndex::<8>::new(metadata, self.bpm)),
            9..=16 => Box::new(BPlusTreeIndex::<16>::new(metadata, self.bpm)),
            17..=32 => Box::new(BPlusTreeIndex::<32>::new(metadata, self.bpm)),
            33..=64 => Box::new(BPlusTreeIndex::<64>::new(metadata, self.bpm)),
            _ => return Err(Exception::new(ExceptionType::NotImplemented, "Unsupported: index key size exceeds 64 bytes")),
        };
        // fill it with what the table already holds (deleted tuples are not indexed)
        for (meta, tuple) in table.table.make_iterator() {
            if !meta.is_deleted {
                index.insert_entry(&tuple.key_from_tuple(&table.schema, &key_schema, &key_attrs), tuple.get_rid());
            }
        }
        let index_oid = self.next_index_oid;
        self.next_index_oid += 1;
        let info = Arc::new(IndexInfo {
            key_schema: (*key_schema).clone(),
            name: index_name.to_owned(),
            index,
            index_oid,
            table_name: table_name.to_owned(),
            key_size,
            is_primary_key,
            index_type: IndexType::BPlusTreeIndex,
        });
        self.indexes.insert(index_oid, info.clone());
        self.index_names.get_mut(table_name).unwrap().insert(index_name.to_owned(), index_oid);
        Ok(Some(info))
        //~ todo!("3c-04: Ok(None) for a missing table or a taken name; an error unless the key is 1 to 64 bytes of INTEGER columns; the index holds every live tuple")
        // @end
    }

    /// The index `index_name` on `table_name`, if there is one.
    pub fn get_index(&self, index_name: &str, table_name: &str) -> Option<Arc<IndexInfo<'a>>> {
        // @begin 3c-04
        let oid = self.index_names.get(table_name)?.get(index_name)?;
        self.indexes.get(oid).cloned()
        //~ todo!("3c-04: the table's index of that name, if any")
        // @end
    }

    /// The index with this `index_oid`, if there is one.
    pub fn get_index_by_oid(&self, index_oid: IndexOid) -> Option<Arc<IndexInfo<'a>>> {
        // @begin 3c-04
        self.indexes.get(&index_oid).cloned()
        //~ todo!("3c-04: look the oid up")
        // @end
    }

    /// Every index of the table (none for an unknown table), in creation order.
    pub fn get_table_indexes(&self, table_name: &str) -> Vec<Arc<IndexInfo<'a>>> {
        // @begin 3c-04
        let Some(names) = self.index_names.get(table_name) else { return vec![] };
        let mut oids: Vec<IndexOid> = names.values().copied().collect();
        oids.sort();
        oids.into_iter().map(|oid| self.indexes[&oid].clone()).collect()
        //~ todo!("3c-04: the table's indexes, ordered by oid")
        // @end
    }
}

#[allow(dead_code)]
type _Unused = GenericKey<4>;
