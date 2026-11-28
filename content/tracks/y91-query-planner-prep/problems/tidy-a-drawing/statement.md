A drawing program saves pictures as trees. After a lot of editing the trees are full of clutter. Write `tidy(shape)`, which returns a simpler tree that draws exactly the same picture.

A drawing is a list of dots, each at a place, in drawing order.

`Dot(n)` draws dot `n` at (0, 0). `Group` draws its parts in order. `Move(x, y, s)` draws `s` shifted by (x, y). `Hidden(s)` draws nothing.

"The same picture" means the same dots at the same places in the same order.

**In the result, none of these may appear anywhere**

1. A `Hidden`. (Something that draws nothing can become `Group(vec![])`.)
2. A group directly inside a group.
3. A group with exactly one part.
4. A move directly inside a move.
5. A move by (0, 0).
6. A move of an empty group.
7. A group with two or more parts that **all** are moves by the same (x, y). Make that move once, around the group.

An empty group may remain only when the whole drawing is empty. Rule 2 rules it out inside a group, rule 6 inside a move.

Change only what these rules need. A tree that already keeps all seven comes back unchanged. So never gather just some of a group's parts under a shared move.

Tidying a result again gives back exactly the same tree.

Trees are at most 200 levels deep. A group may have 3000 parts. All sums of moves fit in an `i64`.
