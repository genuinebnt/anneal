//! Ordering values that may be NULL.

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nulls {
    First,
    Last,
}

/// Compares two nullable integers for one sort key.
pub fn cmp_nullable(a: Option<i64>, b: Option<i64>, dir: Dir, nulls: Nulls) -> Ordering {
    // @begin 3a-c2
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => {
            if nulls == Nulls::First {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (Some(_), None) => {
            if nulls == Nulls::First {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (Some(x), Some(y)) => {
            if dir == Dir::Asc {
                x.cmp(&y)
            } else {
                y.cmp(&x)
            }
        }
    }
    //~ todo!("3a-c2: NULLs by placement, values by direction")
    // @end
}
