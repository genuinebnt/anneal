/// Formats `n` in decimal into the **end** of `buf` and returns the text (a slice of `buf`). No allocation.
/// 20 bytes are enough for any `u64` (`18446744073709551615` has 20 digits).
pub fn fmt_u64(n: u64, buf: &mut [u8; 20]) -> &str {
    todo!()
}

/// The same for `i64`, with a leading `-` for negatives. `-9223372036854775808` has 20 characters and fits.
pub fn fmt_i64(n: i64, buf: &mut [u8; 20]) -> &str {
    todo!()
}

/// `n` with a comma between each group of three digits (`1234567` -> `"1,234,567"`). 26 bytes always suffice.
pub fn fmt_grouped(n: u64, buf: &mut [u8; 26]) -> &str {
    todo!()
}
