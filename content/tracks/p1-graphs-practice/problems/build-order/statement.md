A project has `n` packages numbered `0` to `n - 1`. Each pair `(package, needs)` says that `package` can only be built **after** `needs` has been built.

Return a list of all `n` packages in an order that respects every pair. If no such order exists because the dependencies form a loop, return `None`. When several orders work, any one is fine.

```python
build_order(3, [(1, 0), (2, 1)])        # [0, 1, 2]
build_order(2, [(0, 1), (1, 0)])        # None
```

There can be 20,000 packages in one long chain. That is deeper than Python's recursion limit (about 1,000 calls), so a recursive search fails.
