//! An ordered list of columns, and with it the **layout** of a tuple: a fixed part holding every column in order (an inlined column's
//! bytes, or for a `VARCHAR` a 4-byte offset), followed by the variable part holding the strings.
//!
//! ```text
//! | col 0 (fixed size) | col 1 (VARCHAR: u32 offset) | col 2 (fixed size) | ... | string for col 1 | string for another VARCHAR |
//! |<------------------ inlined storage size = the schema's length ----------->|<-- variable part, in column order -------------->|
//! ```

use std::sync::Arc;

use super::column::Column;

pub type SchemaRef = Arc<Schema>;

#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    /// The size of the fixed part of a tuple.
    length: u32,
    columns: Vec<Column>,
    tuple_is_inlined: bool,
    uninlined_columns: Vec<u32>,
}

impl Schema {
    /// Builds the schema of `columns`, read left to right: each column gets its offset (a VARCHAR takes 4 bytes in the fixed part), the
    /// schema's length is the total, and it remembers which columns are not inlined.
    pub fn new(columns: Vec<Column>) -> Schema {
        todo!("3b-01: each column's offset follows the previous column (a VARCHAR takes 4 bytes); the length is the total; remember which columns are VARCHARs")
    }

    /// A schema of some of `from`'s columns, in the order of `attrs`.
    pub fn copy_schema(from: &Schema, attrs: &[u32]) -> Schema {
        todo!("3b-01: a new schema (new offsets!) of the chosen columns")
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn column(&self, col_idx: u32) -> &Column {
        &self.columns[col_idx as usize]
    }

    /// The index of the first column called `name`, if any.
    pub fn try_col_idx(&self, name: &str) -> Option<u32> {
        todo!("3b-01: the position of the first column with that name")
    }

    /// The index of the first column called `name`.
    ///
    /// # Panics
    ///
    /// Panics if there is none.
    pub fn col_idx(&self, name: &str) -> u32 {
        todo!("3b-01: try_col_idx, or a panic if there is no such column")
    }

    /// The indexes of the VARCHAR columns.
    pub fn uninlined_columns(&self) -> &[u32] {
        &self.uninlined_columns
    }

    pub fn column_count(&self) -> u32 {
        self.columns.len() as u32
    }

    pub fn uninlined_column_count(&self) -> u32 {
        self.uninlined_columns.len() as u32
    }

    /// The size of the fixed part of a tuple of this schema.
    pub fn inlined_storage_size(&self) -> u32 {
        self.length
    }

    /// True if no column is a VARCHAR.
    pub fn is_inlined(&self) -> bool {
        self.tuple_is_inlined
    }

    /// `simplified`: `(a:INTEGER, b:VARCHAR(20))`. Otherwise `Schema[NumColumns:2, IsInlined:0, Length:8] :: (Column[...], ...)`.
    pub fn to_string(&self, simplified: bool) -> String {
        todo!("3b-01: the two formats in the doc comment (IsInlined is 0 or 1)")
    }
}
