**Interview prompt. Time box: 60 minutes** (core in about 20). The tests are named `core_`, `part2_`, `part3_` and
`part4_`. The problem counts as solved when all parts pass.

A dispatcher keeps work items, each under a unique key. **The smallest priority is served first.** Priorities change
while items wait. Do not use `BinaryHeap`, `BTreeMap` or `BTreeSet`.

## Core

`UrgencyQueue<K, P>`:

- `new()`;
- `push(key, priority)` adds a new key and returns `true`. If the key is already waiting it changes nothing and
  returns `false`;
- `pop()` removes and returns the entry with the smallest priority;
- `peek()` shows what `pop` would return;
- `priority(&key)` returns the key's current priority;
- `len()` and `is_empty()` (given).

`push` and `pop` take `O(log n)`, and `priority` and `peek` take expected `O(1)`.

## Part 2: change a priority

`change_priority(&key, priority)` sets a new priority, larger or smaller, and returns the old one. For a key that is
not waiting it returns `None` and adds nothing. It must take `O(log n)`.

**Memory must follow the number of waiting keys, not the number of changes.** After many changes to 100 keys, the
queue may keep at most a few hundred `P` values alive.

## Part 3: cancel

`remove(&key)` takes an entry out from anywhere and returns its priority, in `O(log n)`. The key may be pushed again.

## Part 4: fair ties

Among equal priorities, the key that was **pushed first** is served first. Changing a key's priority does not change
when it was pushed. A key that was popped or removed and is pushed again counts as new.

`K` is `Hash + Eq + Clone` and `P` is `Ord + Clone`. The tests run 400 000 operations on 100 000 keys.
