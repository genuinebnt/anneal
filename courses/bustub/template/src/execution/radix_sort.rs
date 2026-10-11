//! A stable LSD radix sort on u32 keys.

/// Sorts `(key, payload)` pairs by key, stably, in four byte-wise counting-sort passes from the least significant byte: linear in the number of
/// rows.
pub fn radix_sort(rows: &mut Vec<(u32, u32)>) {
    todo!("3g-c5: four stable counting-sort passes, least significant byte first")
}
