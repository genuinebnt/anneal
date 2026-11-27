A 3D tool smooths a surface by first making it finer. Each triangle is cut into four smaller ones, through the **middle of each side**.

The mesh has `points` points, numbered from 0. A triangle is three of those numbers. This function never sees positions. It says which new points are needed, and how everything connects.

`cut_in_four(points, triangles)` returns a `Cut`:

- `halfway` lists the new points. Entry `i` names the two old points it lies halfway between, in either order. That new point has the number `points + i`. The entries may come in any order.
- The surface must have **no cracks**: an edge gets **exactly one** new point, however many triangles use it. No other points are added.
- `triangles` has four pieces for each input triangle. The pieces of input triangle `k` are at places `4k` to `4k + 3`, in any order.
- The four pieces are the usual ones: one at each corner, and one in the middle.
- Each piece **turns the same way** as its parent. If you walk the parent's corners anticlockwise, the piece's corners are anticlockwise too. A piece may start at any of its corners.

Return `None` if any triangle uses a number `>= points` or uses one number twice, or if a new number would not fit in a `usize`.
