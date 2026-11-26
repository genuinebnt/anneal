Join `words` with `sep` between them, without `join` or `concat`. Compute the final length first and
allocate once. The tests count heap allocations: a non-empty result takes exactly one.
