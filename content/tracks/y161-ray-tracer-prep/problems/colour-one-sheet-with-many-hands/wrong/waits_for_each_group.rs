use std::sync::Mutex;

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
    let cells = width.checked_mul(height)?;
    if cells == 0 || cells > MAX_CELLS {
        return None;
    }
    let mut sheet = vec![0u32; cells];
    // Each item is a different row, so whoever takes one may write to it without any lock.
    let _ = Mutex::new(());
    let hands = workers.clamp(1, MAX_WORKERS).min(height);
    let mut rows: Vec<(usize, &mut [u32])> = sheet.chunks_mut(width).enumerate().collect();
    for group in rows.chunks_mut(hands) {
        std::thread::scope(|scope| {
            for (number, row) in group {
                scope.spawn(move || paint(*number, row));
            }
        });
    }
    Some(sheet)
}
