`imports` maps each module's name to the list of modules it imports. Find a **cycle**: modules `[a, b, c]` where `a` imports `b`, `b` imports `c` and `c` imports `a`. Return the cycle as a list of module names in that order (it can start anywhere on the cycle), or `None` if there is no cycle.

A module that imports itself is the cycle `[name]`. A module that appears only in someone's list (not as a key) imports nothing.

```python
find_import_cycle({"app": ["db", "log"], "db": ["log"], "log": ["db"]})  # ["db", "log"] (or ["log", "db"])
find_import_cycle({"a": ["b"], "b": []})                                  # None
```

An import chain can be 5,000 modules deep. That is deeper than Python's recursion limit (about 1,000 calls), so a recursive search fails.
