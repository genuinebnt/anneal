A list of **distinct** numbers was sorted in increasing order, and then its **last** item was moved to the **front**, some number of times. Given the result, return how many times that was done. A list that is still sorted has been rotated `0` times.

```python
rotation_count([4, 5, 6, 1, 2, 3])   # 3: 6, 5, 4 each moved from the back to the front
rotation_count([1, 2, 3])            # 0
rotation_count([2, 3, 1])            # 2
```

The list can have millions of items, and one test passes a sequence of 10^12 items that supports `len()` and indexing like a list.
