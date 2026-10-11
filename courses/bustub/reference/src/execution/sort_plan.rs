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
    // @begin 3g-c1
    if pages == 0 {
        return Some(SortPlan { initial_runs: 0, passes: 0, io_pages: 0 });
    }
    if buffer < 3 {
        return None;
    }
    let initial_runs = pages.div_ceil(buffer);
    let mut runs = initial_runs;
    let mut passes = 1u32;
    while runs > 1 {
        runs = runs.div_ceil(buffer - 1);
        passes += 1;
    }
    Some(SortPlan { initial_runs, passes, io_pages: 2 * pages * passes as u64 })
    //~ todo!("3g-c1: runs after the first pass, then merge passes until one run is left")
    // @end
}
