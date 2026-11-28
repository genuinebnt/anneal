Write `Life::step`. Every cell's next state depends on its neighbours' *old* states, so updating the grid
in place is wrong, and `step` must not allocate either: a test counts allocations. `Life` already
has a second buffer, `next`.
