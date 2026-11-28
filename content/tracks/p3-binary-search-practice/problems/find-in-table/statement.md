A table is a list of rows. Reading it left to right, row by row, gives numbers in **strictly increasing** order. Return the position `(row, col)` of `target`, or `None` if it isn't there.

```python
find_in_table([[1, 3, 5], [7, 9, 11]], 9)    # (1, 1)
find_in_table([[1, 3, 5], [7, 9, 11]], 4)    # None
```

The table can have millions of cells, and one test passes a table of 10^12 cells whose rows are built on request (they support `len()` and indexing like lists).
