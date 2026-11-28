Packages with the given `weights` must be shipped **in order**, one ship per day, and each day's ship carries a consecutive run of packages whose total weight is at most the ship's capacity. You have `days` days (`days >= 1`; `weights` is not empty and every weight is positive). Return the **smallest capacity** that gets everything shipped in time.

```python
min_capacity([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 5)   # 15
min_capacity([3, 2, 2, 4, 1, 4], 3)                # 6
```
