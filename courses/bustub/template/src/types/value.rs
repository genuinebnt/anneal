//! A `Value` is a SQL value: a number, a boolean, a string or **NULL** (which still has a type).
//!
//! * the smallest number of each integer type is reserved as the stored encoding of NULL, so constructing a `Value` with it gives a NULL
//!   (`Value::integer(i32::MIN)` is `NULL`), and the smallest usable value is one above it;
//! * comparisons answer with a three-valued [`CmpBool`] (`NULL = 1` is neither true nor false);
//! * arithmetic with a NULL is NULL; overflow, a bad cast and a division by zero are [`Exception`]s.

use std::fmt;

use super::limits::*;
use super::type_id::TypeId;
use crate::common::exception::{Exception, ExceptionType, Result};

/// A comparison's answer in SQL's three-valued logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpBool {
    False,
    True,
    Null,
}

impl CmpBool {
    pub fn from_bool(b: bool) -> CmpBool {
        if b {
            CmpBool::True
        } else {
            CmpBool::False
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// NULL of a type (`Null(TypeId::Integer)` is the integer NULL; every type has one).
    Null(TypeId),
    Boolean(bool),
    TinyInt(i8),
    SmallInt(i16),
    Integer(i32),
    BigInt(i64),
    Decimal(f64),
    /// A timestamp is carried and compared as a number; dates are not parsed or formatted.
    Timestamp(u64),
    Varchar(String),
}

fn err(kind: ExceptionType, message: impl Into<String>) -> Exception {
    Exception::new(kind, message)
}

fn out_of_range() -> Exception {
    err(ExceptionType::OutOfRange, "Numeric value out of range.")
}

impl Value {
    // ---- 3a-01 · constructing values ----------------------------------------------------------------------------------------

    /// NULL of `type_id`.
    pub fn null(type_id: TypeId) -> Value {
        todo!("3a-01: the NULL of that type")
    }

    /// A BOOLEAN.
    pub fn boolean(b: bool) -> Value {
        todo!("3a-01: a BOOLEAN")
    }

    /// A TINYINT; `BUSTUB_INT8_NULL` (`i8::MIN`) is the stored encoding of NULL, so that number gives a NULL.
    pub fn tinyint(v: i8) -> Value {
        todo!("3a-01: a TINYINT, or the TINYINT NULL for the reserved number i8::MIN")
    }

    /// A SMALLINT; as for [`tinyint`](Self::tinyint), the smallest `i16` is the NULL encoding.
    pub fn smallint(v: i16) -> Value {
        todo!("3a-01: like tinyint, for i16")
    }

    /// An INTEGER; as for [`tinyint`](Self::tinyint), the smallest `i32` is the NULL encoding.
    pub fn integer(v: i32) -> Value {
        todo!("3a-01: like tinyint, for i32")
    }

    /// A BIGINT; as for [`tinyint`](Self::tinyint), the smallest `i64` is the NULL encoding.
    pub fn bigint(v: i64) -> Value {
        todo!("3a-01: like tinyint, for i64")
    }

    /// A DECIMAL; `BUSTUB_DECIMAL_NULL` (the lowest `f64`) is the NULL encoding.
    pub fn decimal(v: f64) -> Value {
        todo!("3a-01: a DECIMAL, or the DECIMAL NULL for the reserved number f64::MIN")
    }

    /// A TIMESTAMP; `u64::MAX` is the TIMESTAMP NULL.
    pub fn timestamp(v: u64) -> Value {
        todo!("3a-01: a TIMESTAMP, or the TIMESTAMP NULL for u64::MAX")
    }

    /// A VARCHAR holding `s`.
    pub fn varchar(s: &str) -> Value {
        todo!("3a-01: a VARCHAR holding the string")
    }

    /// The type of the value; a NULL has the type it was made with.
    pub fn type_id(&self) -> TypeId {
        todo!("3a-01: the type of the value (a NULL has the type it was made with)")
    }

    /// Whether the value is NULL.
    pub fn is_null(&self) -> bool {
        todo!("3a-01: is this a NULL")
    }

    /// Any integer type's value as an `i64` (`None` for NULL and the other types).
    pub fn as_i64(&self) -> Option<i64> {
        todo!("3a-01: Some for TINYINT, SMALLINT, INTEGER and BIGINT values, widened; None otherwise")
    }

    /// Any numeric type's value as an `f64` (`None` for NULL and the non-numeric types).
    pub fn as_f64(&self) -> Option<f64> {
        todo!("3a-01: Some for the numeric types (integers widened to f64); None otherwise")
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Varchar(s) => Some(s),
            _ => None,
        }
    }

    /// Whether `self` can be compared with `other`: a boolean with a boolean or a string, a number with a number or a string, a string with anything, a
    /// timestamp with a timestamp.
    pub fn check_comparable(&self, other: &Value) -> bool {
        todo!("3a-01: the comparability table in the doc comment")
    }

    // ---- 3a-02 · casting ----------------------------------------------------------------------------------------------------

    /// Converts the value to another type.
    /// * Numbers (integers and decimal) convert to each other and to `Varchar` (the number as text). A narrower integer type must be able to hold the
    ///   number (else `OutOfRange`); a decimal truncates toward zero.
    /// * A `Varchar` converts to a boolean (`true`/`1`/`t`, `false`/`0`/`f`, any case), a number (the leading number in the text: `"32"`, `" -7"`,
    ///   `"12abc"` is 12) or itself. No digits is a `Conversion` error.
    /// * A boolean converts to itself and to `Varchar`; a timestamp to itself and to `Varchar`. Anything else is an error ("X is not coercable to Y").
    /// * A NULL converts to the NULL of the target type (if the conversion is allowed at all).
    pub fn cast_as(&self, to: TypeId) -> Result<Value> {
        todo!("3a-02: a NULL becomes the NULL of the target; a number converts to VARCHAR as its text; otherwise the conversion must be allowed and the value must fit the target's range")
    }

    // ---- 3a-03 · comparing --------------------------------------------------------------------------------------------------

    // TODO(3a-03): a private helper of yours (its name and shape are your choice).

    // TODO(3a-03): a private helper of yours (its name and shape are your choice).

    /// `Null` if either is NULL, else whether `self == other`.
    pub fn compare_equals(&self, other: &Value) -> Result<CmpBool> {
        todo!("3a-03: Null if either is NULL, else whether they are equal")
    }

    /// `Null` if either is NULL, else whether `self != other`.
    pub fn compare_not_equals(&self, other: &Value) -> Result<CmpBool> {
        todo!("3a-03: Null if either is NULL, else whether they differ")
    }

    /// `Null` if either is NULL, else whether `self < other`.
    pub fn compare_less_than(&self, other: &Value) -> Result<CmpBool> {
        todo!("3a-03: Null if either is NULL, else self < other")
    }

    /// `Null` if either is NULL, else whether `self <= other`.
    pub fn compare_less_than_equals(&self, other: &Value) -> Result<CmpBool> {
        todo!("3a-03: Null if either is NULL, else self <= other")
    }

    /// `Null` if either is NULL, else whether `self > other`.
    pub fn compare_greater_than(&self, other: &Value) -> Result<CmpBool> {
        todo!("3a-03: Null if either is NULL, else self > other")
    }

    /// `Null` if either is NULL, else whether `self >= other`.
    pub fn compare_greater_than_equals(&self, other: &Value) -> Result<CmpBool> {
        todo!("3a-03: Null if either is NULL, else self >= other")
    }

    /// Equal in the sense of grouping and hashing, not of SQL: two NULLs are equal here.
    pub fn compare_exactly_equals(&self, other: &Value) -> bool {
        todo!("3a-03: true for two NULLs; otherwise whether compare_equals says True (an error counts as false)")
    }

    // ---- 3a-04 · arithmetic -------------------------------------------------------------------------------------------------

    /// What an arithmetic operation with a NULL gives: the NULL of the result type (the wider integer type, or DECIMAL if either side is a decimal).
    pub fn operate_null(&self, other: &Value) -> Result<Value> {
        todo!("3a-04: the NULL of the result type: DECIMAL if either side is, else the wider integer type; a string on the right takes the left's type; non-numeric operands are an error")
    }

    /// Whether a numeric value is zero; any other type is a `NotImplemented` error.
    pub fn is_zero(&self) -> Result<bool> {
        todo!("3a-04: is a numeric value zero; other types are a NotImplemented error")
    }

    // TODO(3a-04): a private helper of yours (its name and shape are your choice).

    // TODO(3a-04): a private helper of yours (its name and shape are your choice).

    /// `self + other`.
    pub fn add(&self, other: &Value) -> Result<Value> {
        todo!("3a-04: self + other")
    }

    /// `self - other`.
    pub fn subtract(&self, other: &Value) -> Result<Value> {
        todo!("3a-04: self - other")
    }

    /// `self * other`.
    pub fn multiply(&self, other: &Value) -> Result<Value> {
        todo!("3a-04: self * other")
    }

    /// `self / other`; integers divide toward zero.
    pub fn divide(&self, other: &Value) -> Result<Value> {
        todo!("3a-04: self / other (integers divide toward zero)")
    }

    /// `self % other`.
    pub fn modulo(&self, other: &Value) -> Result<Value> {
        todo!("3a-04: self % other")
    }

    /// The smaller of the two (a NULL on either side gives a NULL). Works on numbers and strings.
    pub fn min(&self, other: &Value) -> Result<Value> {
        todo!("3a-04: NULL if either is NULL; otherwise the smaller of the two by compare_less_than_equals")
    }

    /// The larger of the two (a NULL on either side gives a NULL).
    pub fn max(&self, other: &Value) -> Result<Value> {
        todo!("3a-04: NULL if either is NULL; otherwise the larger of the two by compare_greater_than_equals")
    }

    // TODO(3a-04): a private helper of yours (its name and shape are your choice).

    /// The square root as a DECIMAL (NULL for NULL). A negative number is a `Decimal` error.
    pub fn sqrt(&self) -> Result<Value> {
        todo!("3a-04: NULL for NULL; a negative number is a Decimal error; otherwise the square root as a DECIMAL")
    }

    // ---- 3a-05 · storage ----------------------------------------------------------------------------------------------------

    /// How many bytes `serialize_to` writes. Fixed-size types: their size. A `Varchar`: a 4-byte length, then the text and a terminating zero byte
    /// (the length counts that zero byte); a NULL string is just the 4-byte marker.
    pub fn storage_size(&self) -> usize {
        todo!("3a-05: the number of bytes serialize_to writes: the type's size, or for VARCHAR 4 + the text + 1 (a NULL VARCHAR: 4)")
    }

    /// Writes the value into `storage` (at least `storage_size()` bytes), little-endian. A NULL is written as its reserved encoding
    /// (`i32::MIN` for an INTEGER, ...; a VARCHAR as the length `u32::MAX`), a boolean as one byte 0 or 1.
    pub fn serialize_to(&self, storage: &mut [u8]) {
        todo!("3a-05: little-endian at the start of storage; a NULL as its reserved encoding; a VARCHAR as a 4-byte length (text plus one), the text and a zero byte")
    }

    /// Reads a value of `type_id` from the start of `storage`. The inverse of `serialize_to`: a reserved encoding is a NULL.
    pub fn deserialize_from(storage: &[u8], type_id: TypeId) -> Result<Value> {
        todo!("3a-05: the inverse of serialize_to: reserved encodings are NULLs; a VARCHAR is a 4-byte length (u32::MAX is NULL) then length minus 1 bytes of text")
    }
}

// TODO(3a-04): private types of yours for the arithmetic go here.

// TODO(3a-02): private helpers for the casts go here.

impl fmt::Display for Value {
    /// Booleans as `true`/`false`, integers in decimal, decimals with six digits after the point (`3.140000`), strings as they are, timestamps as the
    /// number; a NULL as `<type>_null` (`integer_null`, `varlen_null`, ...).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3a-01: the text of the value as the doc comment says")
    }
}
