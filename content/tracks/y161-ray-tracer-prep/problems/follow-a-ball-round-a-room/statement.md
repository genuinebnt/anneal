A small ball rolls over the floor of a room. It moves in straight lines, never slows down, and bounces off the walls. List where it bounces.

Each wall is a straight line: the points with `nx * x + ny * y = offset`. `(nx, ny)` is not zero. It may have **any length** and point to either side.

The room is **convex**: it is the region on the starting point's side of **every** wall. So no wall line passes through the room. `start` is strictly inside, at least `0.01` from every wall.

`bounces(walls, start, velocity, count)` returns the first `count` bounces, in order. Each gives the wall's number in `walls` and the point.

**At a wall** the part of the velocity along the wall stays the same. The part at right angles to the wall turns around.

**In a corner** two walls are reached at the same moment. Report both, at the same point, the **lower number first**. They count as two bounces. Corner tests use numbers where both times come out exactly equal.

**The list ends early** if the ball never reaches another wall: the room has an opening, or `velocity` is zero.

Tests allow an error of `1e-9` on small cases. On random rooms they check each bounce against the one before, within `1e-7`. Some games are 20,000 bounces long, and the ball must still be in the room.
