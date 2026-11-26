A tank in a small plant is worked by a job sheet. Some steps on the sheet have a fixed order, and some may be done in whichever order you like. Write `plan(job, start, capacity)`, which chooses an order that never lets the tank run below empty or overflow.

A job is one of:

- `Add(n)`: an **action** that changes the level by `n`. A negative `n` draws liquid out.
- `Double`: an **action** that doubles the level.
- `InOrder(list)`: do the members in the order written.
- `AnyOrder(list)`: do every member once, in an order you choose.

Groups nest. A member of a group is finished completely before the next member begins, so the actions of two members never mix. A list may be empty, and then there is nothing to do.

The level starts at `start`. After **every** action it must lie in `0..=capacity`. Here `0 <= start <= capacity`, and `capacity` and `n` can be any such `i64`, however large.

**Numbers.** The actions are numbered from 0 in the order they are written on the sheet, left to right. Groups get no number.

**Result.**

- `order` is `Some` list of action numbers: a complete order that the sheet allows and that stays in bounds. If there are several, give the one that is smallest when compared number by number. It is `None` when there is none.
- `furthest` looks at every order the sheet allows, and at how many actions each performs before one would leave the bounds. It is the largest such number. When `order` is `Some`, it is the number of actions.

**Sizes.** A sheet has at most 300 actions. When no complete order exists, it has at most 8. Some larger sheets leave dozens of actions free. In those, always trying the earlier written member first succeeds without ever going back, and your answer must come back at once.
