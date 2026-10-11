//! The cost of an external merge sort.

#[derive(Debug, PartialEq, Eq)]
pub struct SortPlan {
    pub initial_runs: u64,
    pub passes: u32,
    pub io_pages: u64,
}

/// The cost of sorting `pages` pages with `buffer` pages of memory: runs after the first pass, then merge passes with a fan-in of `buffer - 1` until
/// one run is left, and `2 * pages * passes` page transfers. `None` when `buffer < 3`: a merge needs two inputs and an output.
pub fn plan_sort(pages: u64, buffer: u64) -> Option<SortPlan> {
    todo!("3g-c1: runs after the first pass, then merge passes until one run is left")
}
