**Capstone, step 1 of 9.** Over nine steps you build a ray tracer. It follows rays from a camera into a scene and works out what colour comes back. It ends with soft shadows, mirrors, glass, bounced light and a PNG file.

Everything starts with one question: where does a ray first meet a surface?

## Vectors

`Vec3` holds a point, a direction or a colour. Give it `+`, `-`, unary `-`, `* f64` and `/ f64`, and the methods in the starter. `times` multiplies component by component. `unit` is only called on vectors of non-zero length.

## Surfaces

`Shape::hit` and `Plane::hit` return the smallest `t` with `tmin < t < tmax` where `ray.at(t)` lies on the surface. Both ends are **excluded**. `0 <= tmin < tmax`, and `tmax` may be infinite. `dir` is not zero, but it **need not have length 1**.

- **Sphere:** all points at distance `|radius|` from the centre. A ray starting inside hits the far side.
- A **negative radius** describes the same surface, and `normal_at` then points inward.
- **Triangle:** the flat area between three corners, seen from either side. Rays within `1e-9` of an edge are not tested.
- `normal_at` of a triangle is the unit vector of `(b - a) x (c - a)`.
- **Plane:** all points with `normal . p = offset`. `normal` need not have length 1.

A ray exactly parallel to a plane or triangle misses it. A triangle with all corners on one line is never hit. Tests do not use rays that are *nearly* parallel.

**Scale must not matter.** A triangle with sides of `1e-8` works like one with sides of `1e6`.

**Distance must not matter.** A ball of radius 1 seen from `3e8` away is still a ball of radius 1. A ray passing 1.01 from its centre misses. NaN coordinates are never hit and never panic.

## The world

`closest` returns the nearest hit among all prims and planes. Its `normal` is the surface's own unit normal; do **not** turn it towards the ray. Equal `t` goes to the lower prim number, or the lower plane number. A tie between a prim and a plane may go either way.

`blocked` says whether anything is hit inside the window.

Checking every surface in turn is fine here. A later step makes it fast.

Tests compare `t` within `1e-9` (relative for large values) unless they say otherwise.
