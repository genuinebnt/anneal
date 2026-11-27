**Capstone, step 7 of 9.** The starter contains reference code for steps 1 to 6.

Right now every ray is tested against every shape. A mesh of 5,000 triangles makes that hopeless. This step answers the same questions while looking at very little.

## Boxes

`Prim::bounds` returns the lowest and highest corner of the box around the shape, with edges along the axes. A negative radius describes the same ball.

`box_entry(low, high, ray, tmin, tmax)` says whether the ray is inside the box for some `t` in `[tmin, tmax]`, ends **included**. If so, it returns the first such `t`. The faces belong to the box.

- A box may be flat.
- `dir` may have parts that are zero, also `-0.0`. A ray running **along a face** counts.
- It may also say yes to a ray that misses by no more than rounding.

## Searching

`Finder::build(boxes)` prepares for many searches. Solid `i` lies inside `boxes[i]`. The finder never sees the solids or the ray. It asks a `Probe`:

- `enter(low, high, tmin, tmax)` is `box_entry` for the hidden ray. Ask it about **any** box you like.
- `hit(i, tmin, tmax)` tries solid `i`.

`closest` returns the smallest `t` with `tmin < t < tmax` over all solids, with the solid's number. Of equal `t`, the **lowest number** wins. It must be **exactly** the answer of trying every solid. `any` says whether there is a hit at all.

A solid may be hit only if the probe lets you enter its surroundings. If `enter` always says no, you find nothing and never call `hit`.

## How little to ask

The tests count every call to `enter` and `hit` together.

| Scene | Question | Calls allowed |
|---|---|---|
| 4,096 balls in a row along any axis, 3 apart | nearest or any, from either end | 150 |
| the same | a ray that stays clear of the row, or whose window ends before it | 10 |
| 4,096 small balls spread through a cube | nearest, average over rays | 400 |

## Rough input

Many solids may have the **same** box (tests use 20,000). Some may have NaN corners; they are never hit, and they must not hide others or cause a panic. Rays may start `1e8` away.

## The world

Make `World` build a `Finder` for its prims in `new` and use it in `closest` and `blocked`. Their answers do not change.

 Planes have no box, so keep trying them one by one. The tests cannot count calls inside `World`, so this part is graded only by its answers. The next steps need the speed.
