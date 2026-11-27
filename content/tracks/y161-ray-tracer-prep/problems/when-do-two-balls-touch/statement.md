A game needs to know when two round pieces will collide, long before it happens. It cannot afford to move them in tiny steps and look.

Each `Ball` is a disc with its centre at `(x, y)` at time 0. It moves with velocity `(vx, vy)` for ever. `radius` is 0 or more.

`first_touch(a, b)` returns the **first time `t >= 0`** at which the discs touch or overlap. Return `None` if that never happens.

- If they touch or overlap at time 0, the answer is `Some(0.0)`, even if they are separating.
- Grazing counts as touching. The grazing tests use numbers that come out exactly.
- Balls with the same velocity never get closer.

**Far apart must work.** A pair may start `3e9` apart, with radii of 1. A pair that passes 2.01 apart still misses, and a pair that touches is still found.

All inputs are finite. Tests allow an error of `1e-9 * max(1, t)` in the time, except in one case: balls that start `1e-6` apart must get their time within `1e-12`. Random tests skip pairs whose smallest gap is within `1e-5` of touching.
