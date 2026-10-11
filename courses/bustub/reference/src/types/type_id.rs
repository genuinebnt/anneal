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
        // @begin 3a-01
        match self {
            TypeId::Boolean | TypeId::TinyInt => Ok(1),
            TypeId::SmallInt => Ok(2),
            TypeId::Integer => Ok(4),
            TypeId::BigInt | TypeId::Decimal | TypeId::Timestamp => Ok(8),
            TypeId::Varchar => Ok(0),
            TypeId::Invalid => Err(Exception::new(ExceptionType::UnknownType, "Unknown type.")),
        }
        //~ todo!("3a-01: 1 byte for BOOLEAN and TINYINT, 2 for SMALLINT, 4 for INTEGER, 8 for BIGINT, DECIMAL and TIMESTAMP, 0 for VARCHAR; INVALID is an UnknownType error")
        // @end
    }

    /// The SQL name: `"BOOLEAN"`, `"INTEGER"`, ..., `"INVALID"`.
    pub fn type_id_to_string(self) -> &'static str {
        // @begin 3a-01
        match self {
            TypeId::Invalid => "INVALID",
            TypeId::Boolean => "BOOLEAN",
            TypeId::TinyInt => "TINYINT",
            TypeId::SmallInt => "SMALLINT",
            TypeId::Integer => "INTEGER",
            TypeId::BigInt => "BIGINT",
            TypeId::Decimal => "DECIMAL",
            TypeId::Varchar => "VARCHAR",
            TypeId::Timestamp => "TIMESTAMP",
        }
        //~ todo!("3a-01: the upper-case SQL name of the type")
        // @end
    }

    /// Whether a value of type `other` can be converted to this type. `Invalid` accepts nothing; `Boolean` accepts anything; the numeric types accept
    /// the numeric types and `Varchar`; `Timestamp` accepts `Varchar` and itself; `Varchar` accepts every type except `Invalid`.
    pub fn is_coercable_from(self, other: TypeId) -> bool {
        // @begin 3a-01
        use TypeId::*;
        match self {
            Invalid => false,
            Boolean => true,
            TinyInt | SmallInt | Integer | BigInt | Decimal => matches!(other, TinyInt | SmallInt | Integer | BigInt | Decimal | Varchar),
            Timestamp => matches!(other, Varchar | Timestamp),
            Varchar => matches!(other, Boolean | TinyInt | SmallInt | Integer | BigInt | Decimal | Timestamp | Varchar),
        }
        //~ todo!("3a-01: the coercion table in the doc comment")
        // @end
    }

    pub fn is_integer(self) -> bool {
        matches!(self, TypeId::TinyInt | TypeId::SmallInt | TypeId::Integer | TypeId::BigInt)
    }

    pub fn is_numeric(self) -> bool {
        self.is_integer() || self == TypeId::Decimal
    }

    /// The smallest value of the type (`BUSTUB_INT32_MIN`, an empty string, ...). Errors with `MismatchType` for `Invalid`.
    pub fn min_value(self) -> Result<Value> {
        // @begin 3a-01
        use super::limits::*;
        match self {
            TypeId::Boolean => Ok(Value::boolean(false)),
            TypeId::TinyInt => Ok(Value::tinyint(BUSTUB_INT8_MIN)),
            TypeId::SmallInt => Ok(Value::smallint(BUSTUB_INT16_MIN)),
            TypeId::Integer => Ok(Value::integer(BUSTUB_INT32_MIN)),
            TypeId::BigInt => Ok(Value::bigint(BUSTUB_INT64_MIN)),
            TypeId::Decimal => Ok(Value::decimal(BUSTUB_DECIMAL_MIN)),
            TypeId::Timestamp => Ok(Value::timestamp(BUSTUB_TIMESTAMP_MIN)),
            TypeId::Varchar => Ok(Value::varchar("")),
            TypeId::Invalid => Err(Exception::new(ExceptionType::MismatchType, "Cannot get minimal value.")),
        }
        //~ todo!("3a-01: the smallest usable value of each type (one above the reserved NULL encoding for integers); the empty string for VARCHAR; INVALID is a MismatchType error")
        // @end
    }

    /// The largest value of the type (a `VARCHAR`'s is NULL: there is no largest string). Errors with `MismatchType` for `Invalid`.
    pub fn max_value(self) -> Result<Value> {
        // @begin 3a-01
        use super::limits::*;
        match self {
            TypeId::Boolean => Ok(Value::boolean(true)),
            TypeId::TinyInt => Ok(Value::tinyint(BUSTUB_INT8_MAX)),
            TypeId::SmallInt => Ok(Value::smallint(BUSTUB_INT16_MAX)),
            TypeId::Integer => Ok(Value::integer(BUSTUB_INT32_MAX)),
            TypeId::BigInt => Ok(Value::bigint(BUSTUB_INT64_MAX)),
            TypeId::Decimal => Ok(Value::decimal(BUSTUB_DECIMAL_MAX)),
            TypeId::Timestamp => Ok(Value::timestamp(BUSTUB_TIMESTAMP_MAX)),
            TypeId::Varchar => Ok(Value::null(TypeId::Varchar)),
            TypeId::Invalid => Err(Exception::new(ExceptionType::MismatchType, "Cannot get max value.")),
        }
        //~ todo!("3a-01: the largest value of each type; VARCHAR: NULL; INVALID: a MismatchType error")
        // @end
    }
}
