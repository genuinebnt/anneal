Locks make transactions wait, and waiting can go in a circle: T1 holds A and wants B, T2 holds B and wants A. Nothing in the lock manager will ever resolve that, so a database watches for it. This stage builds the **waits-for graph** (an arrow from each waiting transaction to the transactions it waits for), a cycle search that names a **victim**, the manager's `detect_deadlocks` that turns the waiting requests into that graph and aborts the victim of each cycle, and a background thread that runs it.

> [!CHECK] T1 holds S(A); T2 waits for X(A); T3 holds X(B) and asks for S(A); T1 asks for S(B). Draw the waits-for graph with edges to holders only. Is there a cycle? Is there a deadlock?
> ||With holder edges only: T2 waits for T1 (on A) and T1 waits for T3 (on B): no cycle. But T3's S(A) is compatible with the holder T1 and still waits, because the queue is fair and T2's X is ahead of it: T3 waits for T2. So T1 waits for T3, T3 for T2 and T2 for T1, a real deadlock that a holder-only graph never finds.||
>
> - Why does T3 wait at all, if S is compatible with T1's S?
> - Whom does T3 really wait for?
> - Add that edge and look again.

## The task

- `WaitsForGraph` in `src/concurrency/deadlock.rs`: `add_edge`, `remove_edge`, `edge_list` (ordered), and `has_cycle`, which returns the victim: the **youngest** (largest id) transaction on the cycle, found by searching depth first from the smallest id and trying neighbours in increasing id order.
- `LockManager::detect_deadlocks()`: each waiting request waits for the holders it is incompatible with **and**, because the queue is fair (3), for every earlier waiting request it is incompatible with; break each cycle at its youngest transaction, wake the victims, and return them. A victim's waiting `lock` call returns the `Deadlock` error.
- `DeadlockDetector::start` runs `detect_deadlocks` every so often on a thread; dropping the detector stops and joins the thread.

The tests: graphs with no cycle (chains, diamonds); the victim of two-, three- and longer cycles, and of a cycle reached through a tail; removing an edge; two and three transactions that really deadlock and get resolved (the younger loses, the others go on); a deadlock that exists only through queue order; the detector thread doing it alone; a waiting request that is not in a cycle is left alone; and a property: **a cycle is found exactly when a transaction can reach itself, the victim is the youngest on a cycle it belongs to, and removing victims one after another leaves no cycle**.

## Your freedom

The graph's representation; how the victim's waiting call is told to give up; whether the detector is a loop with a sleep or waits on a condition. The graph is built from the lock manager's state while holding its lock, so it is a consistent snapshot.

## The Rust toolbox

**Ordered collections.** A `BTreeMap` of `BTreeSet`s iterates in id order, which fixes the search order for you.

**A stop flag and `Drop`.** `Arc<AtomicBool>` tells the thread to stop; `Drop` sets it and `join`s, so the thread never outlives the detector.

**`Option<JoinHandle>` and `take()`.** `Drop` only has `&mut self`; `take` moves the handle out so it can be joined.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): a background thread with a clean shutdown.
- [S4 Maps & sets](/t/s4-maps-sets): ordered maps.

## Tests

- Cycle-free graphs; victims of cycles of different shapes; edge changes; the ordered edge list.
- Real deadlocks of two and three transactions, and one through queue order; the detector thread; no false alarm.
- Property: cycle detection and victims against brute force.

## Hints

### Searching with a path

Keep the current path. A neighbour that is already on the path closes a cycle; the cycle is the path from there on.

### Break one cycle, then look again

Removing the victim's edges can leave another cycle. Repeat until none is left.

### Build the graph and choose victims under the same lock

If the lock is released in between, a request may have been granted since and the victim would be innocent.

## Performance

The search is `O(V + E)` per cycle, and it runs every few milliseconds over the waiting requests only, which are few. The interval is the trade-off: shorter finds deadlocks sooner and costs CPU; longer lets stuck transactions sit.

**Measure it.** Time `detect_deadlocks` with ten thousand waiting requests and no cycle.

## Experiment

Optional. Predict first, then run.

1. **Choose the oldest instead of the youngest.** Which tests fail, and which transactions would a real system lose more work from?
2. **Run the detector every second.** How long does the two-way deadlock test take?

## Other designs

- **Wait-die and wound-wait** prevent cycles by comparing ages at request time: no graph, no thread, more aborts.
- **Timeouts** give up after a while: simple and imprecise.
- **Edges to holders only** (a smaller graph): correct only for a queue that lets a compatible request past an incompatible waiter. With a fair queue it misses deadlocks like the one in the check question, and those transactions wait for ever.

## In BusTub

The 2022 lock manager runs `RunCycleDetection` on a background thread every 50 ms, builds the waits-for graph from the request queues, aborts the youngest transaction of each cycle and wakes the waiters; its `AddEdge`, `RemoveEdge`, `HasCycle` and `GetEdgeList` are the graph methods here.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::thread` with `std::atomic<bool> enable_cycle_detection_` | a `JoinHandle` and an `Arc<AtomicBool>`, joined in `Drop` |
| `std::unordered_map<txn_id_t, std::vector<txn_id_t>>` | `BTreeMap<TxnId, BTreeSet<TxnId>>` (ordered, no duplicates) |

**Port rule:** a thread you start has an owner that stops it; put that in `Drop`.

## Learn more

- [Wait-for graph (Wikipedia)](https://en.wikipedia.org/wiki/Wait-for_graph) · [PostgreSQL: deadlocks](https://www.postgresql.org/docs/current/explicit-locking.html#LOCKING-DEADLOCKS)
