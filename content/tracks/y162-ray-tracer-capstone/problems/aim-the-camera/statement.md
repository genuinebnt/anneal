**Capstone, step 3 of 9.** The starter contains reference code for steps 1 and 2. This step builds the camera, which decides what rays a picture is made of.

## The camera

The camera sits at `position` and looks towards `look_at`. `up` says which way is up in the picture. It need not be at right angles to the view or have length 1.

The picture is a flat rectangle. `vfov_degrees` is the angle from its bottom edge to its top edge. It is `aspect` times as wide as it is tall.

In `ray(s, t, lens)`, `s` goes from the left edge (0) to the right (1), and `t` from the **top** (0) to the bottom (1). `dir` has length 1.

`new` returns `None` when:

- any number is NaN or infinite;
- `look_at` equals `position`;
- `up` is zero, or so close to the view direction that the sine of the angle between them is at most `1e-9`;
- `vfov_degrees` is not strictly between 0 and 180;
- `aspect <= 0`, `lens_diameter < 0` or `focus_distance <= 0`.

## The lens

`lens` is a point `(x, y)` of the disk of radius 1. The ray starts at `position`, moved by `x * lens_diameter / 2` to the picture's right and `y * lens_diameter / 2` to the picture's up.

All rays with the same `(s, t)` pass through one point. It is where the ray from `position` through `(s, t)` reaches the **sharp plane**. That plane faces the camera, `focus_distance` ahead along the view direction.

With `lens_diameter` 0 this is a pinhole camera.

## Samples inside a pixel

`pixel_offsets(samples, rng)` returns `samples` points `(x, y)` with both parts in `[0, 1)`.

- If `samples` is a square `n * n`: cut the pixel into `n` by `n` equal cells. Put **exactly one point in each cell**, at a random place inside it.
- Otherwise: each point is anywhere in the pixel, at random.

The order of the points and the number of calls to `rng` are free. `rng` returns random numbers in `[0, 1)`.

`unit_disk(rng)` returns a random point of the disk of radius 1. Every region of the disk must be as likely as any other region of the same area.

Tests compare positions and directions within `1e-7` or tighter. Tests of random placement allow five standard deviations.
