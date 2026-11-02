Implement a consistent-hash ring. Each node owns `vnodes` points on a 64-bit circle: the point for virtual node `i` of node `name` is
`hash64("{name}#{i}")` (`hash64` is provided). A key belongs to the node owning the **first point at or after `hash64(key)`**, wrapping from the top
of the circle to the lowest point.

`add_node` / `remove_node` change membership; `get(key)` returns the owner (`None` for an empty ring); `get_n(key, n)` returns up to `n`
**distinct** nodes walking clockwise from the key: the primary and then its replicas.

Properties the tests check: adding a node moves only keys that now belong to the new node, about `1/(n+1)` of them; removing a node moves only that node's keys;
with 200 virtual nodes the load across 10 nodes stays within a factor of about 1.5 of fair.
