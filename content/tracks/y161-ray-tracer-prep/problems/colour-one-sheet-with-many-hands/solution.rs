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
    let rows = Mutex::new(sheet.chunks_mut(width).enumerate());
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, MAX_WORKERS).min(height) {
            scope.spawn(|| loop {
                // The guard lives only to the end of this statement.
                let next = rows
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .next();
                let Some((number, row)) = next else { break };
                paint(number, row);
            });
        }
    });
    Some(sheet)
}
