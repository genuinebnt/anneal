A **peak** is a position whose value is **greater than both neighbours** (a missing neighbour at either end counts as lower than everything). Neighbouring values are never equal. Return the index of **any** peak.

```python
find_a_peak([1, 3, 2])          # 1
find_a_peak([1, 2, 3, 1])       # 2
find_a_peak([1, 2, 3, 4])       # 3
```

The list can have a million items, and one test passes a sequence of 10^12 items that only supports `len()` and indexing, so look at as few as you can.
