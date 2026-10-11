//! The smallest value of each integer type (`INT_MIN`, ...) is reserved as the encoding of SQL NULL, so the smallest *usable* value is one above
//! it. A value constructed with the reserved number is a NULL.

pub const DBL_LOWEST: f64 = f64::MIN;

pub const BUSTUB_INT8_MIN: i8 = i8::MIN + 1;
pub const BUSTUB_INT16_MIN: i16 = i16::MIN + 1;
pub const BUSTUB_INT32_MIN: i32 = i32::MIN + 1;
pub const BUSTUB_INT64_MIN: i64 = i64::MIN + 1;
pub const BUSTUB_DECIMAL_MIN: f64 = f32::MIN as f64;
pub const BUSTUB_TIMESTAMP_MIN: u64 = 0;

pub const BUSTUB_INT8_MAX: i8 = i8::MAX;
pub const BUSTUB_INT16_MAX: i16 = i16::MAX;
pub const BUSTUB_INT32_MAX: i32 = i32::MAX;
pub const BUSTUB_INT64_MAX: i64 = i64::MAX;
pub const BUSTUB_DECIMAL_MAX: f64 = f64::MAX;
pub const BUSTUB_TIMESTAMP_MAX: u64 = 11231999986399999999;

/// The reserved encodings of NULL, as stored in a page.
pub const BUSTUB_VALUE_NULL: u32 = u32::MAX;
pub const BUSTUB_INT8_NULL: i8 = i8::MIN;
pub const BUSTUB_INT16_NULL: i16 = i16::MIN;
pub const BUSTUB_INT32_NULL: i32 = i32::MIN;
pub const BUSTUB_INT64_NULL: i64 = i64::MIN;
pub const BUSTUB_TIMESTAMP_NULL: u64 = u64::MAX;
pub const BUSTUB_DECIMAL_NULL: f64 = DBL_LOWEST;
pub const BUSTUB_BOOLEAN_NULL: i8 = i8::MIN;
