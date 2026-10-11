//! Binary search in a sorted slice that may hold duplicates: the first position a key could be inserted at and the first position after all
//! copies of it.

/// The first index `i` with `slice[i] >= key` (or `slice.len()` if there is none): the number of elements smaller than `key`.
pub fn lower_bound(slice: &[i64], key: i64) -> usize {
    let (mut lo, mut hi) = (0, slice.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if slice[mid] < key {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The first index `i` with `slice[i] > key` (or `slice.len()`): the number of elements smaller than or equal to `key`.
pub fn upper_bound(slice: &[i64], key: i64) -> usize {
    let (mut lo, mut hi) = (0, slice.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        // TODO(2a-c2): this looks right and has a bug; find it and fix it
        if slice[mid] < key {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// How many copies of `key` the slice holds.
pub fn count_of(slice: &[i64], key: i64) -> usize {
    upper_bound(slice, key) - lower_bound(slice, key)
}
