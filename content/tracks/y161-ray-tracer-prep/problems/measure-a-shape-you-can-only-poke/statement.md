You have a shape inside the square from `(0, 0)` to `(1, 1)`. You cannot see it. All you can do is poke a point and hear whether it is inside. You want the shape's area.

## A fair number

`unit_from(raw)` turns a raw random `u64` into a number in `[0, 1)`.

- The result is a multiple of `2^-53`, and it is **never 1.0**.
- Every such multiple comes from the **same number** of raw values.
- A bigger raw value never gives a smaller result.

## Poking

`covered_share(side, inside, raw)` calls `inside(x, y)` **exactly `side * side` times**. It returns the share of calls that said yes. `raw` gives random `u64`s; use as many as you like. Where you poke depends only on `side` and on what `raw` returns.

It must be right **on average**, for any shape: every place in the square is equally likely to be poked.

It must also be **close in every single run**.

Draw a grid of `side` by `side` equal squares. The answer may only be off because of grid squares that the shape's border passes through. So for a smooth shape and `side = 64`, the error is at most `4 / side`, whatever the rolls were. That has to follow from where you poke, not from luck.

Inside a grid square, the place must be random. Its two coordinates must not be tied to each other.

Return `None`, without poking, if `side` is 0 or larger than `MAX_SIDE`.
