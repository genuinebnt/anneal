`merge(sources)` takes any number of iterators, **each already sorted ascending**, and returns one iterator over all
their items in ascending order. Items that compare equal come out **in source order** (source 0 first), and equal items
of one source keep their order. Items need only be `Ord`: no `Clone`, no `Default`.

**Laziness is part of the contract.** Sources may be infinite or expensive:

- `merge` itself pulls **nothing**;
- at any moment, each source has been asked for at most **one more item than the merge has yielded from it**
  (a final `None` counts as an item);
- a source that has returned `None` is **never asked again** (it may not be fused);
- each item costs **O(log k)** comparisons for `k` sources.

`size_hint` must be as tight as the sources allow: its lower bound is the number of items already taken from sources and
not yet yielded, plus the lower bounds of the unfinished sources; the upper bound is the same sum of upper bounds, or
`None` if one of them has none. Once the merge has returned `None`, it keeps returning `None`.

You may change the fields of `Merge`.
