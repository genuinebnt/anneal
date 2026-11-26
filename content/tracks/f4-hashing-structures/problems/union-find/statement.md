**Interview prompt. Time box: 60 minutes** (core in about 20). The tests are named `core_`, `part2_`, `part3_` and
`part4_`. The problem counts as solved when all parts pass.

A network starts with `n` machines numbered `0..n` and no links. Links are added one at a time. Machines joined
directly or through others form a **group**. Every id passed to these methods is a valid id.

## Core

`Groups`:

- `new(n)`;
- `union(a, b)` links two machines. It returns `true` if this joined two different groups;
- `connected(a, b)` says whether they are in one group (every machine is connected to itself);
- `count()` is the number of groups.

The tests run several hundred thousand operations on 200 000 machines, in orders chosen to be unfriendly. Do not
rely on recursion that can grow with `n`.

## Part 2: sizes

- `group_size(a)` is the number of machines in `a`'s group.
- `largest()` is the size of the largest group, 0 when there are no machines.

## Part 3: new machines

`add()` adds one machine in a group of its own and returns its id, which is the next unused number.

## Part 4: try a change, then undo it

- `checkpoint()` returns a mark for the current state.
- `rollback(mark)` restores that state exactly: unions and additions made since are undone.

Checkpoints nest. A mark stays valid until you roll back to an **earlier** mark. Rolling back to the same mark twice
is allowed. A rollback must cost in proportion to what it undoes, not to `n`: one test takes 100 000 checkpoints on a
large network.
