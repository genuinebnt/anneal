Write `tick`, `use_conn` and `drain_closed` for a connection pool. `tick` updates some connections and
removes others in a single pass, recording what it closed in another field of the pool. A test
ticks a large pool, so removing connections one at a time is too slow.
