//! One column of a table: a name, a type and where its bytes are in a tuple.

use crate::types::type_id::TypeId;

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    name: String,
    type_id: TypeId,
    /// The storage size: the type's size, or for a `VARCHAR` its declared maximum length.
    length: u32,
    /// Where the column's bytes start in a tuple; set by the schema that contains the column.
    offset: u32,
}

impl Column {
    /// A column of a fixed-size type.
    ///
    /// # Panics
    ///
    /// Panics for `Varchar` (use [`Column::new_varchar`]) and `Invalid`.
    pub fn new(name: &str, type_id: TypeId) -> Column {
        // @begin 3b-01
        assert_ne!(type_id, TypeId::Varchar, "Wrong constructor for VARCHAR type.");
        let length = type_id.type_size().expect("Cannot get size of invalid type") as u32;
        Column { name: name.to_owned(), type_id, length, offset: 0 }
        //~ todo!("3b-01: remember the name and type; the length is the type's size; a VARCHAR here is a bug (panic); offset 0 until a schema sets it")
        // @end
    }

    /// A `VARCHAR` column holding up to `length` bytes.
    pub fn new_varchar(name: &str, length: u32) -> Column {
        // @begin 3b-01
        Column { name: name.to_owned(), type_id: TypeId::Varchar, length, offset: 0 }
        //~ todo!("3b-01: a VARCHAR column with that maximum length")
        // @end
    }

    /// The same column with another name (`Column::WithColumnName`): the type, length and offset are kept.
    pub fn with_column_name(&self, name: &str) -> Column {
        // @begin 3b-01
        Column { name: name.to_owned(), ..self.clone() }
        //~ todo!("3b-01: a copy with a different name")
        // @end
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The column's storage size: 1, 2, 4 or 8 for the fixed-size types, the declared length for a VARCHAR.
    pub fn storage_size(&self) -> u32 {
        self.length
    }

    /// Where the column starts in a tuple (set by `Schema::new`).
    pub fn offset(&self) -> u32 {
        self.offset
    }

    pub(crate) fn set_offset(&mut self, offset: u32) {
        self.offset = offset;
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Is the column's value stored in the tuple's fixed part? Everything except a VARCHAR.
    pub fn is_inlined(&self) -> bool {
        // @begin 3b-01
        self.type_id != TypeId::Varchar
        //~ todo!("3b-01: true unless the column is a VARCHAR")
        // @end
    }

    /// `simplified`: `name:TYPE` and, for a VARCHAR, `name:VARCHAR(length)`. Otherwise `Column[name, TYPE, Offset:o, Length:l]`.
    pub fn to_string(&self, simplified: bool) -> String {
        // @begin 3b-01
        if simplified {
            let mut s = format!("{}:{}", self.name, self.type_id.type_id_to_string());
            if self.type_id == TypeId::Varchar {
                s.push_str(&format!("({})", self.length));
            }
            s
        } else {
            format!("Column[{}, {}, Offset:{}, Length:{}]", self.name, self.type_id.type_id_to_string(), self.offset, self.length)
        }
        //~ todo!("3b-01: the two formats in the doc comment")
        // @end
    }
}
