//! Sliding-window sums.

/// The sum of the non-NULL values in each row's frame, which reaches `preceding` rows back and `following` rows ahead, clamped to the input. A frame
/// with no non-NULL value gives `None`. Linear in the input, whatever the frame size.
pub fn moving_sum(values: &[Option<i64>], preceding: usize, following: usize) -> Vec<Option<i64>> {
    todo!("3g-c3: prefix sums and a count of non-NULL values; each frame is a difference")
}
