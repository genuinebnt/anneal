A map renderer paints a huge image row by row. Some rows are open sea and take no time. Others are full of city and take very long. Several threads should paint at once.

`paint_sheet(width, height, workers, paint)` returns a sheet of `height` rows with `width` cells each, row after row. Every cell starts as 0. For each row, call `paint(number, cells)` **exactly once**, with that row's `width` cells.

- Return `None` if the sheet would have no cells, or more than `MAX_CELLS`. No size may panic or overflow.
- Paint at most `workers` rows at the same time; 0 means 1.
- Never use more than `MAX_WORKERS` threads for painting, or more threads than there are rows.
- With `workers >= 2`, rows must really be painted at the same time.
- **A slow row holds up nobody.** While one row is still being painted, free workers go on with the others, until only that row is left.
- If `paint` panics, `paint_sheet` panics. It must not hang or return a half-painted sheet.
- The sheet does not depend on the number of workers.

You may paint on the calling thread or on new threads.

## How the tests force this

Some `paint` functions **wait** inside a row until other rows have been painted, or until several rows have started. A design that cannot do the awaited work meanwhile would wait for ever. The tests give up after 8 seconds and report it. A correct design never waits for anything that is not already on its way, so its speed does not matter.

The calls may come in any order.
