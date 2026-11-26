# Building: A cache / Redis-like server



Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Protocol and loop

**Goal:** Talk to clients.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`resp_protocol`](../../examples/networking/resp_protocol.rs) · advanced | RESP. |
| 1.2 | [`kqueue_epoll_raw`](../../examples/networking/kqueue_epoll_raw.rs) · advanced | Readiness. |
| 1.3 | [`mio_event_loop`](../../examples/networking/mio_event_loop.rs) · advanced | Event loop. |
| 1.4 | [`tcp_framed_server`](../../examples/networking/tcp_framed_server.rs) · advanced | Framing. |

### Step 2: Data and eviction

**Goal:** What lives in memory.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`open_addressing_map`](../../examples/data_structures/open_addressing_map.rs) · advanced | Hash table. |
| 2.2 | [`skiplist`](../../examples/data_structures/skiplist.rs) · advanced | Sorted sets. |
| 2.3 | [`hyperloglog`](../../examples/data_structures/hyperloglog.rs) · advanced | Cardinality. |
| 2.4 | [`lru_cache`](../../examples/data_structures/lru_cache.rs) · advanced | Eviction. |
| 2.5 | [`cache_policy_zoo`](../../examples/data_structures/cache_policy_zoo.rs) · advanced | Policies. |
| 2.6 | [`timing_wheel`](../../examples/os_and_storage/timing_wheel.rs) · advanced | TTL expiry. |

### Step 3: Durability

**Goal:** Survive restarts.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`wal_append_log`](../../examples/os_and_storage/wal_append_log.rs) · advanced | AOF. |
| 3.2 | [`snapshot_cow_rcu`](../../examples/concurrency/snapshot_cow_rcu.rs) · advanced | Snapshots. |
| 3.3 | [`mini_redis`](../../examples/capstones/mini_redis.rs) · capstone | Capstone. |
