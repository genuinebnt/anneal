/// The keys that `name` is filed under.
pub fn keys(name: &str) -> Vec<String> {
    todo!()
}

/// Every name within `slips` slips of `typed`, closest first, as `(number, slips needed)`.
/// `list(key)` fetches the numbers filed under a key; `compare(number)` is the expensive check.
pub fn near(
    typed: &str,
    slips: usize,
    list: &mut dyn FnMut(&str) -> Vec<usize>,
    compare: &mut dyn FnMut(usize) -> Option<usize>,
) -> Vec<(usize, usize)> {
    todo!()
}
