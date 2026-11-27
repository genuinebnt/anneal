Light comes to you through a row of rooms. Room `i` **glows** with some light of its own, and it **passes** on a share, from 0 to 1, of the light that reaches it from the next room. The light you receive from room 0 is:

```text
glow0 + passes0 * (glow1 + passes1 * (glow2 + ...))
```

The row may be endless. So you walk it only part of the way, and use chance to decide where to stop. `one_estimate` returns **one** random estimate. The average over many runs must be **exactly** the true total.

`room(i)` returns `(glow, passes)` for room `i`, or `None` past the end. Ask for rooms in order: 0, 1, 2, and so on. Stop at the first `None`.

## The rules of chance

So that the tests can follow your walk, the rolls are fixed:

- Rooms 0 and 1 never roll. After rooms 0 and 1, always ask for the next one.
- After taking in room `i >= 2`, call `roll()` **exactly once**. It returns a number in `[0, 1)`.
- Let `w` be the share of light from room `i + 1` that your walk now counts. With no rolling, that is `passes0 * ... * passes_i`.
- Let `p` be `w` limited to between `0.05` and `0.95`.
- **Go on** if `roll() < p`. Otherwise stop, and ask for no more rooms.

Make no other calls to `roll`, also when `w` is 0.

What you must work out is what going on means for the numbers. In the rule for `w`, "your walk now counts" includes any change you made after earlier rolls.

The tests add up **every** possible outcome with its chance, and compare with the true total within `1e-9`.
