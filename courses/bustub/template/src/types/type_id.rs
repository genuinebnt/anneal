//! The SQL types and what is true of each type as a whole: its size, its name, which types it can be converted from.

use super::value::Value;
use crate::common::exception::{Exception, ExceptionType, Result};

/// Every SQL type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeId {
    Invalid = 0,
    Boolean,
    TinyInt,
    SmallInt,
    Integer,
    BigInt,
    Decimal,
    Varchar,
    Timestamp,
}

impl TypeId {
    /// The size of this type in bytes in a tuple's fixed part (a `VARCHAR`'s is 0: its bytes live elsewhere). `Invalid` is an error (`UnknownType`).
    pub fn type_size(self) -> Result<u64> {
        todo!("3a-01: 1 byte for BOOLEAN and TINYINT, 2 for SMALLINT, 4 for INTEGER, 8 for BIGINT, DECIMAL and TIMESTAMP, 0 for VARCHAR; INVALID is an UnknownType error")
    }

    /// The SQL name: `"BOOLEAN"`, `"INTEGER"`, ..., `"INVALID"`.
    pub fn type_id_to_string(self) -> &'static str {
        todo!("3a-01: the upper-case SQL name of the type")
    }

    /// Whether a value of type `other` can be converted to this type. `Invalid` accepts nothing; `Boolean` accepts anything; the numeric types accept
    /// the numeric types and `Varchar`; `Timestamp` accepts `Varchar` and itself; `Varchar` accepts every type except `Invalid`.
    pub fn is_coercable_from(self, other: TypeId) -> bool {
        todo!("3a-01: the coercion table in the doc comment")
    }

    pub fn is_integer(self) -> bool {
        matches!(self, TypeId::TinyInt | TypeId::SmallInt | TypeId::Integer | TypeId::BigInt)
    }

    pub fn is_numeric(self) -> bool {
        self.is_integer() || self == TypeId::Decimal
    }

    /// The smallest value of the type (`BUSTUB_INT32_MIN`, an empty string, ...). Errors with `MismatchType` for `Invalid`.
    pub fn min_value(self) -> Result<Value> {
        todo!("3a-01: the smallest usable value of each type (one above the reserved NULL encoding for the integers); VARCHAR: the empty string; INVALID: a MismatchType error")
    }

    /// The largest value of the type (a `VARCHAR`'s is NULL: there is no largest string). Errors with `MismatchType` for `Invalid`.
    pub fn max_value(self) -> Result<Value> {
        todo!("3a-01: the largest value of each type; VARCHAR: NULL; INVALID: a MismatchType error")
    }
}
