//! Comparing plain integers as index keys.

use std::cmp::Ordering;

use super::generic_key::KeyComparator;

/// Compares plain integers.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntComparator;

impl KeyComparator<i32> for IntComparator {
    fn compare(&self, lhs: &i32, rhs: &i32) -> Ordering {
        todo!("2a-02: the natural order of the integers")
    }
}
