//! DISTINCT over rows of nullable integers.

use std::collections::HashSet;

pub type Row = Vec<Option<i64>>;

/// The rows without repeats, in first-seen order. NULL equals NULL here, as in `DISTINCT`.
pub fn distinct_rows(rows: &[Row]) -> Vec<Row> {
    todo!("3b-c5: keep the first of each distinct row, treating NULL as equal to NULL")
}

/// The number of distinct rows.
pub fn distinct_count(rows: &[Row]) -> usize {
    todo!("3b-c5: how many distinct rows")
}
