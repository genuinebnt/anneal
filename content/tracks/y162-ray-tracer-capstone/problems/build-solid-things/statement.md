**Capstone, step 2 of 9.** The starter contains reference code for step 1. It is a stand-in until the app can carry your own code over. This step builds solid things out of triangles.

For a closed solid, list each triangle's corners **anticlockwise as seen from outside**. Then `(b - a) x (c - a)` points outwards. Every triangle gets the given material number.

## Quad

`quad(a, b, c, d, material)` returns the triangles `(a, b, c)` and `(a, c, d)`, with the corners in that order.

## Box

`cuboid` returns 12 triangles, in any order. The box is `size.x` wide, `size.y` tall and `size.z` deep, with positive sides. `base` is the middle of its bottom face.

`yaw` turns it about the upright line through `base`. A point at `(x, z)` from that line moves to `(x cos + z sin, z cos - x sin)`. All normals point outwards.

## Ball

`icosphere` approximates a ball with flat triangles.

- **Level 0** is the regular icosahedron. Its corners are the 12 points `(±1, ±g, 0)`, `(0, ±1, ±g)` and `(±g, 0, ±1)`, with `g = (1 + √5) / 2`, scaled onto the ball. It has 20 equal faces.
- **Each further level** replaces every triangle by four. Their corners are the old corners and the middle of each side, pushed straight out from the centre onto the ball.

So level `n` has `20 * 4^n` triangles and `10 * 4^n + 2` different corners. The surface has **no cracks**: neighbours share their corners.

Return `None` if `levels` is above `MAX_LEVELS`, or if `radius` is not a finite positive number. Refuse before doing the work: `levels` may be `u32::MAX`.

Tests treat corners within `1e-9` as the same point.
