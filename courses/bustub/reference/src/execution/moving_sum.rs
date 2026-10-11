//! Sliding-window sums.

/// The sum of the non-NULL values in each row's frame, which reaches `preceding` rows back and `following` rows ahead, clamped to the input. A frame
/// with no non-NULL value gives `None`. Linear in the input, whatever the frame size.
pub fn moving_sum(values: &[Option<i64>], preceding: usize, following: usize) -> Vec<Option<i64>> {
    // @begin 3g-c3
    let n = values.len();
    // prefix[i] = sum of the non-NULL values of rows 0..i; nonnull[i] = how many there are
    let mut prefix = vec![0i64; n + 1];
    let mut nonnull = vec![0usize; n + 1];
    for (i, v) in values.iter().enumerate() {
        prefix[i + 1] = prefix[i].wrapping_add(v.unwrap_or(0));
        nonnull[i + 1] = nonnull[i] + usize::from(v.is_some());
    }
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(preceding);
            let hi = (i + following).min(n - 1);
            if nonnull[hi + 1] == nonnull[lo] {
                None
            } else {
                Some(prefix[hi + 1].wrapping_sub(prefix[lo]))
            }
        })
        .collect()
    //~ todo!("3g-c3: prefix sums and a count of non-NULL values; each frame is a difference")
    // @end
}
