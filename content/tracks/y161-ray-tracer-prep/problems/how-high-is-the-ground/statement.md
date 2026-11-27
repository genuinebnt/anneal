A game's landscape is made of triangles. A walker stands somewhere on the map, and the game must put its feet on the ground.

A point is `[x, y, z]`, where `y` is the height. A triangle is three numbers of points.

`height_at(points, triangles, x, z)` looks **straight down** at the place `(x, z)`. It returns the number of the triangle that covers it, and the ground's height there. The ground is flat inside each triangle.

- If several triangles cover the place, take the **lowest number**.
- Edges and corners belong to the triangle. They still count when rounding puts them a hair outside: allow each of the three corner shares of the place to be as low as `-1e-9`.
- Corners may be listed in either order.
- A triangle with **no area seen from above** covers nothing. A standing wall is one. "No area" means the sine of the angle between two sides is below about `1e-12`. Small triangles, with sides of `1e-7`, are real ground.
- A triangle that names a point that does not exist is ignored. NaN coordinates cover nothing. Nothing may panic.

Return `None` if no triangle covers the place. Tests compare heights within `1e-9`.
