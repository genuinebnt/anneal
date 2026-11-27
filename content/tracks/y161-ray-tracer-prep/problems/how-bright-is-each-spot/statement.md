You are lighting a gallery. On a floor plan you know where the lamps and the thin walls are. For any spot on a wall or a display, you want to know how much light falls on it.

A spot is at `spot` and faces along `facing`, which is not zero and may have **any length**.

`brightness` adds up, over every lamp:

```text
power * cosine / distance^2
```

`cosine` is the cosine of the angle between `facing` and the direction from the spot to the lamp. A lamp adds **nothing** if:

- the cosine is 0 or less;
- a wall blocks it;
- it stands exactly on the spot.

## Walls

A wall blocks a lamp if it **crosses the straight way** from the spot to the lamp, between the two ends. A wall lying exactly along that way does not block, and neither does a wall of no length.

- A crossing within `1e-7` of the spot or of the lamp does **not** block. A crossing farther than `0.001` from both does.
- Spots are often **on** a wall, with rounded coordinates. That wall must never shade its own spot.
- Tests do not aim a way within `1e-4` of a wall's end.

Tests compare within `1e-9`.
