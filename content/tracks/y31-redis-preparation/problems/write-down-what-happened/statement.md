A theatre sells seats. A customer first **holds** a seat for a short while, then buys it. The seat is chosen at random, and a hold that is not bought in time lapses. The system must come back exactly as it was after a restart.

The log cannot be a list of the requests: replaying them would need the old dice and the old clock. So it records **what changed**.

**`run(commands, now, dice)`** runs the commands as one unit, and returns one reply per command, and the log entries for this call.

1. First, every hold with `until <= now` lapses: its seat is freed, in seat order. This happens even with no commands, and only here, at the start.
2. Then each command runs in order. Each person has at most one seat, held or sold.

    - `Hold`: refused if `who` has a seat, or none is free. Otherwise ask `dice.below(f)` once, with `f` free seats. Take that free seat, counting from 0 in seat order.
      It is held until `now + ms`, at most `u64::MAX`.
    - `Buy`: refused unless `who` **holds** a seat; it becomes sold.
    - `Release`: refused unless `who` has a seat; it becomes free.

A reply is `Done(seat)` or `Refused`. Dice are thrown only for a hold that succeeds.

**The log of a call.** Each change is one `Change(effect)`, in the order made. A freed lapse is a change too.

With **two or more** changes, put `Begin` before them and `End` after them. With none, return no entries.

**`replay(size, log)`** returns the hall the log describes, and how many entries it used. It needs no dice and no clock.

- A single change outside `Begin`..`End` is a unit by itself. A unit is applied **whole or not at all**. A complete unit counts even if it is empty.
- A log cut short may end inside a unit. That unit is ignored, its changes unchecked, and `used` stops before its `Begin`.
- `Err(BadLog { at })` is for a wrong entry, and `at` is its index. These are wrong:
    - `Begin` inside a unit, or `End` outside one;
    - in a **complete** unit, a change that does not fit. It may name a seat out of range, or hold a taken seat.
    - So are: holding for someone who has a seat, selling a seat that is not held, and freeing a free one.

For any history, replaying the first `n` entries gives the hall as it was after the last whole call within them.
