A database that only knows "locked" and "unlocked" makes readers queue behind each other. This module builds the other answer, a lock manager with several **modes**, and this first stage is its vocabulary: five modes, which of them can be held on the same thing at the same time, and which can replace which. Everything later (queues, upgrades, isolation levels) is a question put to this table.

## The task

In `src/concurrency/lock_mode.rs`:

- `compatible(a, b)`: can two different transactions hold `a` and `b` on the same thing together? Two intention-shared locks can; an exclusive lock can be held with nothing.
- `can_upgrade(from, to)`: may a transaction that holds `from` ask for `to` in its place? Only a stronger mode, never the same one, never a weaker one.

The modes are shared (S), exclusive (X), and the three intention modes IS, IX and SIX that a table lock uses to announce what its rows will be locked with. The concept article has the table.

The tests: the whole compatibility table checked pair by pair against the textbook, the special cases (X with nothing; IS with everything but X; readers and writers), the whole upgrade relation, and two properties: **compatibility is symmetric**, and **an upgrade only strengthens** (whatever fits the new mode already fitted the old one, and there is no way back).

## Your freedom

How the table is written: a `match`, a lookup table, bit sets per mode. The enum is given.

## The Rust toolbox

**`matches!`.** `matches!(x, A | B)` is a boolean test of a pattern; a compatibility rule is often a few of them.

**Exhaustive `match`.** With five modes there are twenty-five pairs. A `match` over a pair with a final `_` arm is short, but a new mode would silently fall into it: decide which you prefer.

## If this is new

- L7 Enums & pattern matching: matching on a pair.

## Tests

- The whole table against the textbook; X with nothing; IS with everything but X; readers against intending writers.
- The upgrade relation, pair by pair.
- Properties: symmetry; an upgrade only strengthens.

## Hints

### Start from what each mode keeps out

X keeps out everything. IS keeps out only X. The rest are in between: write down, for each of S, IX and SIX, the modes that conflict with it, and check that if A's list contains B then B's list contains A.

### Upgrades are about the set of things a mode keeps out

A stronger mode keeps out at least everything the weaker one does. If your upgrade relation lets IX become S, ask what S keeps out that IX does not, and what IX keeps out that S does not.

## Performance

Both functions are `O(1)` on a handful of values; they are called under a lock for every request, so keep them free of allocation.

**Measure it.** Call `compatible` a hundred million times in a loop over all pairs: it should take well under a second.

## Experiment

Optional. Predict first, then run.

1. **Make SIX compatible with IX.** Which of the later stages would break first, and what would a reader of the whole table see?
2. **Allow X to X "upgrades".** Which property fails and what does it print?

## Other designs

- **A lookup table** (`[[bool; 5]; 5]`) is the textbook form and trivially checked.
- **Bit sets:** each mode is the set of modes it conflicts with; compatibility is a bit test.
- **Fewer modes:** a reader-writer lock is two modes (S and X); the three intention modes exist only for a hierarchy of sizes.

## In BusTub

BusTub's lock manager project (the 2022 edition) defines `LockMode { SHARED, EXCLUSIVE, INTENTION_SHARED, INTENTION_EXCLUSIVE, SHARED_INTENTION_EXCLUSIVE }` and the same matrix in its lock manager header and tests.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `enum class LockMode { SHARED, ... }` | `enum LockMode { Shared, ... }` |
| a `bool compat[5][5]` indexed by `int(mode)` | a `match` on the pair, or a table indexed with `as usize` |

**Port rule:** write the rule as data or as a `match` over the pair, and keep it separate from everything that uses it.

## Learn more

- [Gray et al., granularity of locks](https://jimgray.azurewebsites.net/papers/granularitylocks.pdf) · [PostgreSQL table-level locks](https://www.postgresql.org/docs/current/explicit-locking.html)
