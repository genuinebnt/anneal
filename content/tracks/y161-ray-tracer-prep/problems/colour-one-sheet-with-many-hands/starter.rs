/// The most cells a sheet may have.
pub const MAX_CELLS: usize = 1 << 22;
/// The most rows painted at the same time.
pub const MAX_WORKERS: usize = 64;

/// A sheet of `height` rows of `width` cells, each row painted by one call of `paint(row, cells)`.
pub fn paint_sheet(
    width: usize,
    height: usize,
    workers: usize,
    paint: &(dyn Fn(usize, &mut [u32]) + Sync),
) -> Option<Vec<u32>> {
    todo!()
}
