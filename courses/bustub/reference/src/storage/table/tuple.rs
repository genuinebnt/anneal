//! A tuple is a row, stored as bytes, and read back through the [`Schema`] that describes it.
//!
//! ```text
//! | fixed part: one slot per column (inlined bytes, or a u32 offset for a VARCHAR) | variable part: each VARCHAR as length + text |
//! ```
//! NULLs need no extra bytes: a NULL is a reserved pattern in its column's slot.

use crate::catalog::schema::Schema;
use crate::common::rid::Rid;
use crate::types::limits::BUSTUB_VALUE_NULL;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

/// What a table keeps beside each tuple: a timestamp (the transaction that wrote it, or 0 outside transactions) and whether the tuple is deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TupleMeta {
    pub ts: i64,
    pub is_deleted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tuple {
    /// Where the tuple lives in a table (`Rid::default()`, an invalid id, for a tuple not in a table).
    rid: Rid,
    data: Vec<u8>,
}

impl Tuple {
    /// An empty tuple with an invalid record id.
    pub fn empty() -> Tuple {
        Tuple { rid: Rid::default(), data: Vec::new() }
    }

    /// A tuple at `rid` whose bytes are to be filled in by reading a page.
    pub fn with_rid(rid: Rid) -> Tuple {
        Tuple { rid, data: Vec::new() }
    }

    /// A tuple at `rid` that is a copy of `data`.
    pub fn from_bytes(rid: Rid, data: &[u8]) -> Tuple {
        Tuple { rid, data: data.to_vec() }
    }

    /// Builds the tuple holding `values`, one per column of `schema`: the fixed part first (each inlined value at its column's offset;
    /// for a VARCHAR, the offset where its bytes will be), then each VARCHAR's bytes in column order. Panics if there is not one value
    /// of the right type per column.
    pub fn new(values: &[Value], schema: &Schema) -> Tuple {
        // @begin 3b-02
        assert_eq!(values.len(), schema.column_count() as usize, "a tuple needs one value per column");
        for (i, value) in values.iter().enumerate() {
            assert_eq!(value.type_id(), schema.column(i as u32).type_id(), "value {i} is not of its column's type");
        }
        // 1. the size: the fixed part and, for each VARCHAR, its serialised bytes
        let mut tuple_size = schema.inlined_storage_size() as usize;
        for &i in schema.uninlined_columns() {
            tuple_size += values[i as usize].storage_size();
        }
        // 2. write every value
        let mut data = vec![0u8; tuple_size];
        let mut offset = schema.inlined_storage_size() as usize;
        for (i, value) in values.iter().enumerate() {
            let column = schema.column(i as u32);
            if column.is_inlined() {
                value.serialize_to(&mut data[column.offset() as usize..]);
            } else {
                data[column.offset() as usize..column.offset() as usize + 4].copy_from_slice(&(offset as u32).to_le_bytes());
                value.serialize_to(&mut data[offset..]);
                offset += value.storage_size();
            }
        }
        Tuple { rid: Rid::default(), data }
        //~ todo!("3b-02: the fixed part holds each inlined value at its column's offset and, for a VARCHAR, the offset of its bytes; the VARCHAR bytes follow in column order")
        // @end
    }

    pub fn get_rid(&self) -> Rid {
        self.rid
    }

    pub fn set_rid(&mut self, rid: Rid) {
        self.rid = rid;
    }

    /// The tuple's bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut Vec<u8> {
        &mut self.data
    }

    /// The tuple's length in bytes, strings included.
    pub fn get_length(&self) -> u32 {
        self.data.len() as u32
    }

    /// The value of column `column_idx`. An inlined column is read where it is; a VARCHAR through the offset stored in its slot.
    pub fn get_value(&self, schema: &Schema, column_idx: u32) -> Value {
        // @begin 3b-02
        let column = schema.column(column_idx);
        let at = if column.is_inlined() {
            column.offset() as usize
        } else {
            let slot = column.offset() as usize;
            u32::from_le_bytes(self.data[slot..slot + 4].try_into().unwrap()) as usize
        };
        Value::deserialize_from(&self.data[at..], column.type_id()).expect("a column has a valid type")
        //~ todo!("3b-02: an inlined column is read at its offset; a VARCHAR through the offset stored in its slot")
        // @end
    }

    /// Whether the column's value is NULL.
    pub fn is_null(&self, schema: &Schema, column_idx: u32) -> bool {
        // @begin 3b-02
        self.get_value(schema, column_idx).is_null()
        //~ todo!("3b-02: is the column's value NULL")
        // @end
    }

    /// The key tuple of an index: the values of the columns `key_attrs` of this tuple, arranged by `key_schema`.
    pub fn key_from_tuple(&self, schema: &Schema, key_schema: &Schema, key_attrs: &[u32]) -> Tuple {
        // @begin 3b-02
        let values: Vec<Value> = key_attrs.iter().map(|&i| self.get_value(schema, i)).collect();
        Tuple::new(&values, key_schema)
        //~ todo!("3b-02: read the key columns' values, then build a tuple of them under key_schema")
        // @end
    }

    /// `(1, hello, <NULL>)`: the values separated by `, ` in parentheses, a NULL as `<NULL>`.
    pub fn to_string(&self, schema: &Schema) -> String {
        // @begin 3b-02
        let parts: Vec<String> = (0..schema.column_count())
            .map(|i| if self.is_null(schema, i) { "<NULL>".to_string() } else { self.get_value(schema, i).to_string() })
            .collect();
        format!("({})", parts.join(", "))
        //~ todo!("3b-02: the format in the doc comment")
        // @end
    }

    /// Writes the tuple at the start of `storage`: a 4-byte length (little-endian), then the bytes.
    pub fn serialize_to(&self, storage: &mut [u8]) {
        // @begin 3b-02
        storage[..4].copy_from_slice(&(self.data.len() as i32).to_le_bytes());
        storage[4..4 + self.data.len()].copy_from_slice(&self.data);
        //~ todo!("3b-02: a 4-byte length, then the tuple's bytes")
        // @end
    }

    /// Reads a tuple written by `serialize_to` (a copy of the bytes; the record id is not stored).
    pub fn deserialize_from(storage: &[u8]) -> Tuple {
        // @begin 3b-02
        let size = u32::from_le_bytes(storage[..4].try_into().unwrap()) as usize;
        Tuple::from_bytes(Rid::default(), &storage[4..4 + size])
        //~ todo!("3b-02: read the length, then copy that many bytes")
        // @end
    }
}

#[allow(dead_code)]
const _: (u32, TypeId) = (BUSTUB_VALUE_NULL, TypeId::Invalid);
