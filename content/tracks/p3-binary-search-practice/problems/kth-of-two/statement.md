`a` and `b` are sorted lists (possibly empty, possibly **huge**). Return the **k-th smallest** value among all their items together, counting from `k = 1` for the smallest. `1 <= k <= len(a) + len(b)`.

```python
kth_of_two([1, 3, 5], [2, 4, 6], 4)    # 4
kth_of_two([], [7, 8], 2)              # 8
kth_of_two([1, 1, 1], [1, 1], 5)       # 1
```

Merging the lists is too slow: the tests pass sequences of hundreds of millions of items (`range` objects, which support `len()` and indexing like lists).
