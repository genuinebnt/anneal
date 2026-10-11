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
        // @begin 3a-01
        Value::Null(type_id)
        //~ todo!("3a-01: the NULL of that type")
        // @end
    }

    /// A BOOLEAN.
    pub fn boolean(b: bool) -> Value {
        // @begin 3a-01
        Value::Boolean(b)
        //~ todo!("3a-01: a BOOLEAN")
        // @end
    }

    /// A TINYINT; `BUSTUB_INT8_NULL` (`i8::MIN`) is the stored encoding of NULL, so that number gives a NULL.
    pub fn tinyint(v: i8) -> Value {
        // @begin 3a-01
        if v == BUSTUB_INT8_NULL {
            Value::Null(TypeId::TinyInt)
        } else {
            Value::TinyInt(v)
        }
        //~ todo!("3a-01: a TINYINT, or the TINYINT NULL for the reserved number i8::MIN")
        // @end
    }

    /// A SMALLINT; as for [`tinyint`](Self::tinyint), the smallest `i16` is the NULL encoding.
    pub fn smallint(v: i16) -> Value {
        // @begin 3a-01
        if v == BUSTUB_INT16_NULL {
            Value::Null(TypeId::SmallInt)
        } else {
            Value::SmallInt(v)
        }
        //~ todo!("3a-01: like tinyint, for i16")
        // @end
    }

    /// An INTEGER; as for [`tinyint`](Self::tinyint), the smallest `i32` is the NULL encoding.
    pub fn integer(v: i32) -> Value {
        // @begin 3a-01
        if v == BUSTUB_INT32_NULL {
            Value::Null(TypeId::Integer)
        } else {
            Value::Integer(v)
        }
        //~ todo!("3a-01: like tinyint, for i32")
        // @end
    }

    /// A BIGINT; as for [`tinyint`](Self::tinyint), the smallest `i64` is the NULL encoding.
    pub fn bigint(v: i64) -> Value {
        // @begin 3a-01
        if v == BUSTUB_INT64_NULL {
            Value::Null(TypeId::BigInt)
        } else {
            Value::BigInt(v)
        }
        //~ todo!("3a-01: like tinyint, for i64")
        // @end
    }

    /// A DECIMAL; `BUSTUB_DECIMAL_NULL` (the lowest `f64`) is the NULL encoding.
    pub fn decimal(v: f64) -> Value {
        // @begin 3a-01
        if v == BUSTUB_DECIMAL_NULL {
            Value::Null(TypeId::Decimal)
        } else {
            Value::Decimal(v)
        }
        //~ todo!("3a-01: a DECIMAL, or the DECIMAL NULL for the reserved number f64::MIN")
        // @end
    }

    /// A TIMESTAMP; `u64::MAX` is the TIMESTAMP NULL.
    pub fn timestamp(v: u64) -> Value {
        // @begin 3a-01
        if v == BUSTUB_TIMESTAMP_NULL {
            Value::Null(TypeId::Timestamp)
        } else {
            Value::Timestamp(v)
        }
        //~ todo!("3a-01: a TIMESTAMP, or the TIMESTAMP NULL for u64::MAX")
        // @end
    }

    /// A VARCHAR holding `s`.
    pub fn varchar(s: &str) -> Value {
        // @begin 3a-01
        Value::Varchar(s.to_owned())
        //~ todo!("3a-01: a VARCHAR holding the string")
        // @end
    }

    /// The type of the value; a NULL has the type it was made with.
    pub fn type_id(&self) -> TypeId {
        // @begin 3a-01
        match self {
            Value::Null(t) => *t,
            Value::Boolean(_) => TypeId::Boolean,
            Value::TinyInt(_) => TypeId::TinyInt,
            Value::SmallInt(_) => TypeId::SmallInt,
            Value::Integer(_) => TypeId::Integer,
            Value::BigInt(_) => TypeId::BigInt,
            Value::Decimal(_) => TypeId::Decimal,
            Value::Timestamp(_) => TypeId::Timestamp,
            Value::Varchar(_) => TypeId::Varchar,
        }
        //~ todo!("3a-01: the type of the value (a NULL has the type it was made with)")
        // @end
    }

    /// Whether the value is NULL.
    pub fn is_null(&self) -> bool {
        // @begin 3a-01
        matches!(self, Value::Null(_))
        //~ todo!("3a-01: is this a NULL")
        // @end
    }

    /// Any integer type's value as an `i64` (`None` for NULL and the other types).
    pub fn as_i64(&self) -> Option<i64> {
        // @begin 3a-01
        match self {
            Value::TinyInt(v) => Some(*v as i64),
            Value::SmallInt(v) => Some(*v as i64),
            Value::Integer(v) => Some(*v as i64),
            Value::BigInt(v) => Some(*v),
            _ => None,
        }
        //~ todo!("3a-01: Some for TINYINT, SMALLINT, INTEGER and BIGINT values, widened; None otherwise")
        // @end
    }

    /// Any numeric type's value as an `f64` (`None` for NULL and the non-numeric types).
    pub fn as_f64(&self) -> Option<f64> {
        // @begin 3a-01
        match self {
            Value::Decimal(v) => Some(*v),
            other => other.as_i64().map(|v| v as f64),
        }
        //~ todo!("3a-01: Some for the numeric types (integers widened to f64); None otherwise")
        // @end
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
        // @begin 3a-01
        use TypeId::*;
        match self.type_id() {
            Boolean => matches!(other.type_id(), Boolean | Varchar),
            TinyInt | SmallInt | Integer | BigInt | Decimal => matches!(other.type_id(), TinyInt | SmallInt | Integer | BigInt | Decimal | Varchar),
            Varchar => true,
            Timestamp => other.type_id() == Timestamp,
            Invalid => false,
        }
        //~ todo!("3a-01: the comparability table in the doc comment")
        // @end
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
        // @begin 3a-02
        use TypeId::*;
        let from = self.type_id();
        let allowed = match from {
            TinyInt | SmallInt | Integer | BigInt | Decimal => matches!(to, TinyInt | SmallInt | Integer | BigInt | Decimal | Varchar),
            Boolean => matches!(to, Boolean | Varchar),
            Varchar => matches!(to, Boolean | TinyInt | SmallInt | Integer | BigInt | Decimal | Varchar),
            Timestamp => matches!(to, Timestamp | Varchar),
            Invalid => false,
        };
        if !allowed {
            return Err(err(ExceptionType::Invalid, format!("{} is not coercable to {}", from.type_id_to_string(), to.type_id_to_string())));
        }
        if self.is_null() {
            return Ok(Value::Null(to));
        }
        if to == Varchar {
            return Ok(Value::Varchar(self.to_string()));
        }
        match self {
            Value::Boolean(_) | Value::Timestamp(_) => Ok(self.clone()),
            Value::Varchar(s) => cast_text(s, to),
            Value::Decimal(d) => cast_decimal(*d, to),
            _ => cast_integer(self.as_i64().unwrap(), to),
        }
        //~ todo!("3a-02: a NULL becomes the NULL of the target; a number converts to VARCHAR as its text; otherwise the conversion must be allowed and the value must fit the target's range")
        // @end
    }

    // ---- 3a-03 · comparing --------------------------------------------------------------------------------------------------

    // @begin 3a-03
    /// Compares `self` with `other`: `None` if either is NULL, otherwise the ordering. A number and a string compare after the string
    /// is converted to the number's type; a boolean and a string after the string is converted to a boolean; a string and a number (or
    /// anything else) after the other value is converted to text. Incomparable types are an error.
    fn compare(&self, other: &Value) -> Result<Option<std::cmp::Ordering>> {
        if !self.check_comparable(other) {
            return Err(err(ExceptionType::Invalid, "type error"));
        }
        if self.is_null() || other.is_null() {
            return Ok(None);
        }
        use std::cmp::Ordering;
        Ok(match (self, other) {
            (Value::Varchar(a), _) => {
                let b = other.cast_as(TypeId::Varchar)?;
                Some(a.as_bytes().cmp(b.as_str().unwrap().as_bytes()))
            }
            (Value::Boolean(a), _) => Some(a.cmp(&other.cast_as(TypeId::Boolean)?.as_bool().unwrap())),
            (Value::Timestamp(a), Value::Timestamp(b)) => Some(a.cmp(b)),
            (a, Value::Varchar(_)) => return a.compare(&other.cast_as(a.type_id())?),
            (a, b) => match (a.as_i64(), b.as_i64()) {
                (Some(x), Some(y)) => Some(x.cmp(&y)),
                _ => a.as_f64().unwrap().partial_cmp(&b.as_f64().unwrap()).or(Some(Ordering::Equal)),
            },
        })
    }

    //~ // TODO(3a-03): a private helper of yours (its name and shape are your choice).
    // @end

    // @begin 3a-03
    fn cmp_with(&self, other: &Value, f: impl Fn(std::cmp::Ordering) -> bool) -> Result<CmpBool> {
        Ok(match self.compare(other)? {
            None => CmpBool::Null,
            Some(ord) => CmpBool::from_bool(f(ord)),
        })
    }

    //~ // TODO(3a-03): a private helper of yours (its name and shape are your choice).
    // @end

    /// `Null` if either is NULL, else whether `self == other`.
    pub fn compare_equals(&self, other: &Value) -> Result<CmpBool> {
        // @begin 3a-03
        self.cmp_with(other, |o| o.is_eq())
        //~ todo!("3a-03: Null if either is NULL, else whether they are equal")
        // @end
    }

    /// `Null` if either is NULL, else whether `self != other`.
    pub fn compare_not_equals(&self, other: &Value) -> Result<CmpBool> {
        // @begin 3a-03
        self.cmp_with(other, |o| o.is_ne())
        //~ todo!("3a-03: Null if either is NULL, else whether they differ")
        // @end
    }

    /// `Null` if either is NULL, else whether `self < other`.
    pub fn compare_less_than(&self, other: &Value) -> Result<CmpBool> {
        // @begin 3a-03
        self.cmp_with(other, |o| o.is_lt())
        //~ todo!("3a-03: Null if either is NULL, else self < other")
        // @end
    }

    /// `Null` if either is NULL, else whether `self <= other`.
    pub fn compare_less_than_equals(&self, other: &Value) -> Result<CmpBool> {
        // @begin 3a-03
        self.cmp_with(other, |o| o.is_le())
        //~ todo!("3a-03: Null if either is NULL, else self <= other")
        // @end
    }

    /// `Null` if either is NULL, else whether `self > other`.
    pub fn compare_greater_than(&self, other: &Value) -> Result<CmpBool> {
        // @begin 3a-03
        self.cmp_with(other, |o| o.is_gt())
        //~ todo!("3a-03: Null if either is NULL, else self > other")
        // @end
    }

    /// `Null` if either is NULL, else whether `self >= other`.
    pub fn compare_greater_than_equals(&self, other: &Value) -> Result<CmpBool> {
        // @begin 3a-03
        self.cmp_with(other, |o| o.is_ge())
        //~ todo!("3a-03: Null if either is NULL, else self >= other")
        // @end
    }

    /// Equal in the sense of grouping and hashing, not of SQL: two NULLs are equal here.
    pub fn compare_exactly_equals(&self, other: &Value) -> bool {
        // @begin 3a-03
        if self.is_null() && other.is_null() {
            return true;
        }
        self.compare_equals(other).map(|c| c == CmpBool::True).unwrap_or(false)
        //~ todo!("3a-03: true for two NULLs; otherwise whether compare_equals says True (an error counts as false)")
        // @end
    }

    // ---- 3a-04 · arithmetic -------------------------------------------------------------------------------------------------

    /// What an arithmetic operation with a NULL gives: the NULL of the result type (the wider integer type, or DECIMAL if either side is a decimal).
    pub fn operate_null(&self, other: &Value) -> Result<Value> {
        // @begin 3a-04
        let (a, b) = (self.type_id(), other.type_id());
        let b = if b == TypeId::Varchar { a } else { b };
        if !a.is_numeric() || !b.is_numeric() {
            return Err(err(ExceptionType::Invalid, "type error"));
        }
        Ok(Value::Null(wider(a, b)))
        //~ todo!("3a-04: the NULL of the result type: DECIMAL if either side is, else the wider integer type; a string on the right takes the left's type; non-numeric operands are an error")
        // @end
    }

    /// Whether a numeric value is zero; any other type is a `NotImplemented` error.
    pub fn is_zero(&self) -> Result<bool> {
        // @begin 3a-04
        match self {
            Value::Decimal(d) => Ok(*d == 0.0),
            v if v.type_id().is_integer() => Ok(v.as_i64() == Some(0)),
            _ => Err(err(ExceptionType::NotImplemented, "isZero not implemented")),
        }
        //~ todo!("3a-04: is a numeric value zero; other types are a NotImplemented error")
        // @end
    }

    // @begin 3a-04
    fn arithmetic(&self, other: &Value, op: Op) -> Result<Value> {
        let (a, b) = (self.type_id(), other.type_id());
        if !a.is_numeric() || !self.check_comparable(other) {
            return Err(err(ExceptionType::NotImplemented, format!("{:?} is not implemented for {}", op, a.type_id_to_string())));
        }
        if self.is_null() || other.is_null() {
            return self.operate_null(other);
        }
        if op.divides() && other.is_zero_after_cast(a)? {
            return Err(err(ExceptionType::DivideByZero, "Division by zero on right-hand side"));
        }
        let other = if b == TypeId::Varchar { other.cast_as(a)? } else { other.clone() };
        let result_type = wider(a, other.type_id());
        if result_type == TypeId::Decimal {
            let (x, y) = (self.as_f64().unwrap(), other.as_f64().unwrap());
            return Ok(Value::Decimal(match op {
                Op::Add => x + y,
                Op::Subtract => x - y,
                Op::Multiply => x * y,
                Op::Divide => x / y,
                Op::Modulo => x - (x / y).trunc() * y,
            }));
        }
        let (x, y) = (self.as_i64().unwrap() as i128, other.as_i64().unwrap() as i128);
        let exact = match op {
            Op::Add => x + y,
            Op::Subtract => x - y,
            Op::Multiply => x * y,
            Op::Divide => x / y,
            Op::Modulo => x % y,
        };
        cast_integer(i64::try_from(exact).map_err(|_| out_of_range())?, result_type)
    }

    //~ // TODO(3a-04): a private helper of yours (its name and shape are your choice).
    // @end

    // @begin 3a-04
    fn is_zero_after_cast(&self, left: TypeId) -> Result<bool> {
        if self.type_id() == TypeId::Varchar {
            return self.cast_as(left)?.is_zero();
        }
        self.is_zero()
    }

    //~ // TODO(3a-04): a private helper of yours (its name and shape are your choice).
    // @end

    /// `self + other`.
    pub fn add(&self, other: &Value) -> Result<Value> {
        // @begin 3a-04
        self.arithmetic(other, Op::Add)
        //~ todo!("3a-04: self + other")
        // @end
    }

    /// `self - other`.
    pub fn subtract(&self, other: &Value) -> Result<Value> {
        // @begin 3a-04
        self.arithmetic(other, Op::Subtract)
        //~ todo!("3a-04: self - other")
        // @end
    }

    /// `self * other`.
    pub fn multiply(&self, other: &Value) -> Result<Value> {
        // @begin 3a-04
        self.arithmetic(other, Op::Multiply)
        //~ todo!("3a-04: self * other")
        // @end
    }

    /// `self / other`; integers divide toward zero.
    pub fn divide(&self, other: &Value) -> Result<Value> {
        // @begin 3a-04
        self.arithmetic(other, Op::Divide)
        //~ todo!("3a-04: self / other (integers divide toward zero)")
        // @end
    }

    /// `self % other`.
    pub fn modulo(&self, other: &Value) -> Result<Value> {
        // @begin 3a-04
        self.arithmetic(other, Op::Modulo)
        //~ todo!("3a-04: self % other")
        // @end
    }

    /// The smaller of the two (a NULL on either side gives a NULL). Works on numbers and strings.
    pub fn min(&self, other: &Value) -> Result<Value> {
        // @begin 3a-04
        if self.is_null() || other.is_null() {
            return self.operate_null_or_varchar(other);
        }
        Ok(if self.compare_less_than_equals(other)? == CmpBool::True { self.clone() } else { other.clone() })
        //~ todo!("3a-04: NULL if either is NULL; otherwise the smaller of the two by compare_less_than_equals")
        // @end
    }

    /// The larger of the two (a NULL on either side gives a NULL).
    pub fn max(&self, other: &Value) -> Result<Value> {
        // @begin 3a-04
        if self.is_null() || other.is_null() {
            return self.operate_null_or_varchar(other);
        }
        Ok(if self.compare_greater_than_equals(other)? == CmpBool::True { self.clone() } else { other.clone() })
        //~ todo!("3a-04: NULL if either is NULL; otherwise the larger of the two by compare_greater_than_equals")
        // @end
    }

    // @begin 3a-04
    fn operate_null_or_varchar(&self, other: &Value) -> Result<Value> {
        if self.type_id() == TypeId::Varchar {
            Ok(Value::Null(TypeId::Varchar))
        } else {
            self.operate_null(other)
        }
    }

    //~ // TODO(3a-04): a private helper of yours (its name and shape are your choice).
    // @end

    /// The square root as a DECIMAL (NULL for NULL). A negative number is a `Decimal` error.
    pub fn sqrt(&self) -> Result<Value> {
        // @begin 3a-04
        if self.is_null() {
            return Ok(Value::Null(TypeId::Decimal));
        }
        let x = self.as_f64().ok_or_else(|| err(ExceptionType::NotImplemented, "Sqrt not implemented"))?;
        if x < 0.0 {
            return Err(err(ExceptionType::Decimal, "Cannot take square root of a negative number."));
        }
        Ok(Value::Decimal(x.sqrt()))
        //~ todo!("3a-04: NULL for NULL; a negative number is a Decimal error; otherwise the square root as a DECIMAL")
        // @end
    }

    // ---- 3a-05 · storage ----------------------------------------------------------------------------------------------------

    /// How many bytes `serialize_to` writes. Fixed-size types: their size. A `Varchar`: a 4-byte length, then the text and a terminating zero byte
    /// (the length counts that zero byte); a NULL string is just the 4-byte marker.
    pub fn storage_size(&self) -> usize {
        // @begin 3a-05
        match self {
            Value::Varchar(s) => 4 + s.len() + 1,
            Value::Null(TypeId::Varchar) => 4,
            other => other.type_id().type_size().unwrap_or(0) as usize,
        }
        //~ todo!("3a-05: the number of bytes serialize_to writes: the type's size, or for VARCHAR 4 + the text + 1 (a NULL VARCHAR: 4)")
        // @end
    }

    /// Writes the value into `storage` (at least `storage_size()` bytes), little-endian. A NULL is written as its reserved encoding
    /// (`i32::MIN` for an INTEGER, ...; a VARCHAR as the length `u32::MAX`), a boolean as one byte 0 or 1.
    pub fn serialize_to(&self, storage: &mut [u8]) {
        // @begin 3a-05
        match self {
            Value::Boolean(b) => storage[0] = *b as u8,
            Value::TinyInt(v) => storage[..1].copy_from_slice(&v.to_le_bytes()),
            Value::SmallInt(v) => storage[..2].copy_from_slice(&v.to_le_bytes()),
            Value::Integer(v) => storage[..4].copy_from_slice(&v.to_le_bytes()),
            Value::BigInt(v) => storage[..8].copy_from_slice(&v.to_le_bytes()),
            Value::Decimal(v) => storage[..8].copy_from_slice(&v.to_le_bytes()),
            Value::Timestamp(v) => storage[..8].copy_from_slice(&v.to_le_bytes()),
            Value::Varchar(s) => {
                storage[..4].copy_from_slice(&(s.len() as u32 + 1).to_le_bytes());
                storage[4..4 + s.len()].copy_from_slice(s.as_bytes());
                storage[4 + s.len()] = 0;
            }
            Value::Null(t) => match t {
                TypeId::Boolean => storage[0] = BUSTUB_BOOLEAN_NULL as u8,
                TypeId::TinyInt => storage[..1].copy_from_slice(&BUSTUB_INT8_NULL.to_le_bytes()),
                TypeId::SmallInt => storage[..2].copy_from_slice(&BUSTUB_INT16_NULL.to_le_bytes()),
                TypeId::Integer => storage[..4].copy_from_slice(&BUSTUB_INT32_NULL.to_le_bytes()),
                TypeId::BigInt => storage[..8].copy_from_slice(&BUSTUB_INT64_NULL.to_le_bytes()),
                TypeId::Decimal => storage[..8].copy_from_slice(&BUSTUB_DECIMAL_NULL.to_le_bytes()),
                TypeId::Timestamp => storage[..8].copy_from_slice(&BUSTUB_TIMESTAMP_NULL.to_le_bytes()),
                TypeId::Varchar => storage[..4].copy_from_slice(&BUSTUB_VALUE_NULL.to_le_bytes()),
                TypeId::Invalid => {}
            },
        }
        //~ todo!("3a-05: little-endian at the start of storage; a NULL as its reserved encoding; a VARCHAR as a 4-byte length (text plus one), the text and a zero byte")
        // @end
    }

    /// Reads a value of `type_id` from the start of `storage`. The inverse of `serialize_to`: a reserved encoding is a NULL.
    pub fn deserialize_from(storage: &[u8], type_id: TypeId) -> Result<Value> {
        // @begin 3a-05
        let word = |n: usize| -> &[u8] { &storage[..n] };
        Ok(match type_id {
            TypeId::Boolean => match storage[0] as i8 {
                BUSTUB_BOOLEAN_NULL => Value::Null(TypeId::Boolean),
                0 => Value::Boolean(false),
                _ => Value::Boolean(true),
            },
            TypeId::TinyInt => Value::tinyint(i8::from_le_bytes(word(1).try_into().unwrap())),
            TypeId::SmallInt => Value::smallint(i16::from_le_bytes(word(2).try_into().unwrap())),
            TypeId::Integer => Value::integer(i32::from_le_bytes(word(4).try_into().unwrap())),
            TypeId::BigInt => Value::bigint(i64::from_le_bytes(word(8).try_into().unwrap())),
            TypeId::Decimal => Value::decimal(f64::from_le_bytes(word(8).try_into().unwrap())),
            TypeId::Timestamp => Value::timestamp(u64::from_le_bytes(word(8).try_into().unwrap())),
            TypeId::Varchar => {
                let len = u32::from_le_bytes(word(4).try_into().unwrap());
                if len == BUSTUB_VALUE_NULL {
                    Value::Null(TypeId::Varchar)
                } else {
                    let bytes = &storage[4..4 + len as usize - 1]; // the stored length counts the terminating zero byte
                    Value::Varchar(String::from_utf8_lossy(bytes).into_owned())
                }
            }
            TypeId::Invalid => return Err(err(ExceptionType::UnknownType, "Unknown type.")),
        })
        //~ todo!("3a-05: the inverse of serialize_to: reserved encodings are NULLs; a VARCHAR is a 4-byte length (u32::MAX is NULL) then length minus 1 bytes of text")
        // @end
    }
}

// @begin 3a-04
#[derive(Clone, Copy, Debug)]
enum Op {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

impl Op {
    fn divides(self) -> bool {
        matches!(self, Op::Divide | Op::Modulo)
    }
}

/// The type an operation on two numbers produces: DECIMAL if either is, else the wider integer type.
fn wider(a: TypeId, b: TypeId) -> TypeId {
    if a == TypeId::Decimal || b == TypeId::Decimal {
        TypeId::Decimal
    } else {
        a.max(b)
    }
}
//~ // TODO(3a-04): private types of yours for the arithmetic go here.
// @end

// @begin 3a-02
/// Converts an integer to `to` (an integer type, DECIMAL), checking that the target can hold it (the reserved NULL number cannot be a value).
fn cast_integer(v: i64, to: TypeId) -> Result<Value> {
    let in_range = |min: i64, max: i64| if (min..=max).contains(&v) { Ok(()) } else { Err(out_of_range()) };
    match to {
        TypeId::TinyInt => in_range(BUSTUB_INT8_MIN as i64, BUSTUB_INT8_MAX as i64).map(|_| Value::TinyInt(v as i8)),
        TypeId::SmallInt => in_range(BUSTUB_INT16_MIN as i64, BUSTUB_INT16_MAX as i64).map(|_| Value::SmallInt(v as i16)),
        TypeId::Integer => in_range(BUSTUB_INT32_MIN as i64, BUSTUB_INT32_MAX as i64).map(|_| Value::Integer(v as i32)),
        TypeId::BigInt => in_range(BUSTUB_INT64_MIN, BUSTUB_INT64_MAX).map(|_| Value::BigInt(v)),
        TypeId::Decimal => Ok(Value::Decimal(v as f64)),
        _ => Err(err(ExceptionType::Invalid, "not an integer type")),
    }
}

fn cast_decimal(d: f64, to: TypeId) -> Result<Value> {
    let below_or_above = |min: f64, max: f64| d > max || d < min;
    match to {
        TypeId::Decimal => Ok(Value::Decimal(d)),
        TypeId::TinyInt if below_or_above(BUSTUB_INT8_MIN as f64, BUSTUB_INT8_MAX as f64) => Err(out_of_range()),
        TypeId::SmallInt if below_or_above(BUSTUB_INT16_MIN as f64, BUSTUB_INT16_MAX as f64) => Err(out_of_range()),
        TypeId::Integer if below_or_above(BUSTUB_INT32_MIN as f64, BUSTUB_INT32_MAX as f64) => Err(out_of_range()),
        TypeId::BigInt if d >= BUSTUB_INT64_MAX as f64 || d < BUSTUB_INT64_MIN as f64 => Err(out_of_range()),
        _ => cast_integer(d.trunc() as i64, to),
    }
}

/// Converts a string to `to`: the number or boolean written at its start.
fn cast_text(s: &str, to: TypeId) -> Result<Value> {
    match to {
        TypeId::Varchar => Ok(Value::Varchar(s.to_owned())),
        TypeId::Boolean => match s.to_lowercase().as_str() {
            "true" | "1" | "t" => Ok(Value::Boolean(true)),
            "false" | "0" | "f" => Ok(Value::Boolean(false)),
            _ => Err(err(ExceptionType::Invalid, "Boolean value format error.")),
        },
        TypeId::Decimal => {
            let text = leading_number(s, true).ok_or_else(|| err(ExceptionType::Conversion, format!("Cannot convert '{s}' to a number")))?;
            let d: f64 = text.parse().map_err(|_| err(ExceptionType::Conversion, format!("Cannot convert '{s}' to a number")))?;
            if d > BUSTUB_DECIMAL_MAX || d < BUSTUB_DECIMAL_MIN {
                return Err(out_of_range());
            }
            Ok(Value::Decimal(d))
        }
        _ => {
            let text = leading_number(s, false).ok_or_else(|| err(ExceptionType::Conversion, format!("Cannot convert '{s}' to a number")))?;
            let v: i64 = text.parse().map_err(|_| out_of_range())?;
            cast_integer(v, to)
        }
    }
}

/// The text of the number at the start of `s` after optional white space: an optional sign and digits (and, for decimals, a fraction and an
/// exponent). `None` if there are no digits. Like C++'s `std::stoi` / `std::stod`, anything after the number is ignored.
fn leading_number(s: &str, decimal: bool) -> Option<&str> {
    let s = s.trim_start();
    let bytes = s.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if decimal && i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == digits_start || (decimal && i == digits_start + 1 && bytes[digits_start] == b'.') {
        return None;
    }
    if decimal && i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    Some(&s[..i])
}
//~ // TODO(3a-02): private helpers for the casts go here.
// @end

impl fmt::Display for Value {
    /// Booleans as `true`/`false`, integers in decimal, decimals with six digits after the point (`3.140000`), strings as they are, timestamps as the
    /// number; a NULL as `<type>_null` (`integer_null`, `varlen_null`, ...).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3a-01
        match self {
            Value::Null(t) => write!(
                f,
                "{}",
                match t {
                    TypeId::Boolean => "boolean_null",
                    TypeId::TinyInt => "tinyint_null",
                    TypeId::SmallInt => "smallint_null",
                    TypeId::Integer => "integer_null",
                    TypeId::BigInt => "bigint_null",
                    TypeId::Decimal => "decimal_null",
                    TypeId::Timestamp => "timestamp_null",
                    TypeId::Varchar => "varlen_null",
                    TypeId::Invalid => "invalid_null",
                }
            ),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::TinyInt(v) => write!(f, "{v}"),
            Value::SmallInt(v) => write!(f, "{v}"),
            Value::Integer(v) => write!(f, "{v}"),
            Value::BigInt(v) => write!(f, "{v}"),
            Value::Decimal(v) => write!(f, "{v:.6}"),
            Value::Timestamp(v) => write!(f, "{v}"),
            Value::Varchar(s) => write!(f, "{s}"),
        }
        //~ todo!("3a-01: the text of the value as the doc comment says")
        // @end
    }
}
