Join `parts` with `sep` between them. The result must be allocated exactly once:
its capacity must equal its length. The tests count heap allocations, so building the string
in several steps and calling `shrink_to_fit` at the end doesn't pass.
