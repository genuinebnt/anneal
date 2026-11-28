Split the `items` (each a whole-number weight, `0` or more) into **two groups**, every item going to exactly one of them, so that the **difference between the groups' total weights is as small as possible**. Return that smallest difference.

```python
smallest_difference([3, 1, 4, 2, 2, 1])   # 1
smallest_difference([3, 3, 2, 2, 2])      # 0: {3,3} against {2,2,2}
smallest_difference([])                   # 0
```
