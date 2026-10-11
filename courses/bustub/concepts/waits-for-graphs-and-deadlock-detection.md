---
title: Waits-for graphs and deadlock detection
summary: How a database notices that transactions are stuck waiting for each other, what a cycle in a graph of waiting means, how a victim is chosen, and why detection beats prevention for most systems.
minutes: 10
---
Transaction 1 holds a lock on A and wants B. Transaction 2 holds B and wants A. Neither can ever go on: this is a **deadlock**. Nobody made a mistake; locking in an order that depends on the data (a transfer from X to Y and one from Y to X) is enough. A database must *expect* deadlocks and deal with them.

## The graph

Draw one node per transaction and an arrow from T to U when **T is waiting for a lock that U holds**. That is the **waits-for graph**. A transaction that is not waiting has no outgoing arrow. The key fact:

> There is a deadlock exactly when the waits-for graph has a **cycle**.

A chain (1 waits for 2, 2 waits for 3, 3 is running) is not a deadlock: 3 will finish and the chain unwinds. A cycle never unwinds, because every transaction in it is waiting for another transaction in it.

```svg
caption: A waits-for graph with a deadlock. T1 waits for T2, T2 for T3, T3 for T1: a cycle, and nothing in it can move. The detector picks the youngest transaction of the cycle (T3, the largest id) as the victim: its request fails and its locks are released. T2 now runs, T1 and T4 wait for it, and there is no cycle. (Animated: find, choose, break.)
<svg viewBox="0 0 760 330" role="img" aria-label="Four transactions as nodes with waits-for arrows, a cycle among three of them, and the youngest chosen as the victim">
<style>
@keyframes wf-1{0%,28%{opacity:1}34%,100%{opacity:.25}}
@keyframes wf-2{0%,30%{opacity:.25}36%,60%{opacity:1}66%,100%{opacity:.25}}
@keyframes wf-3{0%,62%{opacity:.25}68%,94%{opacity:1}100%{opacity:.25}}
@keyframes wf-v{0%,32%{opacity:0}38%,62%{opacity:1}68%,100%{opacity:0}}
@keyframes wf-e{0%,64%{opacity:1}70%,94%{opacity:.12}100%{opacity:1}}
.wf-1{animation:wf-1 12s ease-in-out infinite}.wf-2{animation:wf-2 12s ease-in-out infinite}.wf-3{animation:wf-3 12s ease-in-out infinite}
.wf-v{opacity:0;animation:wf-v 12s ease-in-out infinite}.wf-e{animation:wf-e 12s ease-in-out infinite}
</style>
<defs>
<marker id="wf-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker>
<marker id="wf-w" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--warn)"/></marker>
</defs>
<path class="ln" d="M590 70 H476" marker-end="url(#wf-a)"/><text class="mid dim sm" x="533" y="60">T4 waits for T2</text>
<g class="wf-e">
<path style="stroke:var(--warn);stroke-width:2;fill:none" d="M220 70 H300" marker-end="url(#wf-w)"/>
<path style="stroke:var(--warn);stroke-width:2;fill:none" d="M368 112 L330 178" marker-end="url(#wf-w)"/>
<path style="stroke:var(--warn);stroke-width:2;fill:none" d="M262 190 L196 112" marker-end="url(#wf-w)"/>
</g>
<rect class="blue" x="140" y="42" width="80" height="56" rx="6"/><text class="mid t-b" x="180" y="66">T1</text><text class="mid dim sm" x="180" y="84">id 1</text>
<rect class="blue" x="300" y="42" width="80" height="56" rx="6"/><text class="mid t-b" x="340" y="66">T2</text><text class="mid dim sm" x="340" y="84">id 2</text>
<rect class="blue" x="590" y="42" width="80" height="56" rx="6"/><text class="mid t-b" x="630" y="66">T4</text><text class="mid dim sm" x="630" y="84">id 4</text>
<rect class="blue" x="230" y="180" width="80" height="56" rx="6"/><text class="mid t-b" x="270" y="204">T3</text><text class="mid dim sm" x="270" y="222">id 3</text>
<g class="wf-v"><rect class="bad" x="226" y="176" width="88" height="64" rx="8" style="fill:none;stroke-width:3"/><text class="mid t-r sm" x="270" y="258">victim: the youngest</text></g>
<text class="t-w sm wf-1" x="30" y="290">1 · find: T1 waits for T2, T2 for T3, T3 for T1. A cycle never unwinds.</text>
<text class="t-r sm wf-2" x="30" y="308">2 · choose: the youngest transaction of the cycle, T3 (the largest id), becomes the victim and is aborted.</text>
<text class="t-g sm wf-3" x="30" y="326">3 · break: its locks are released, T2 runs, T1 and T4 wait for it. Search again: no cycle is left.</text>
</svg>
```

## Detect, then abort one

The usual approach is **detection**: a background thread wakes up every few milliseconds, builds the graph from the lock queues, looks for cycles, and for each cycle chooses a **victim**, which is aborted: its waiting request fails, it gives back its locks, and the others go on. The choice of victim is a policy; the common one is the **youngest** transaction in the cycle (the one with the largest id), because it has done the least work to throw away. A deterministic rule matters in practice: with a fixed search order the same deadlock always loses the same transaction, which makes behaviour testable.

Breaking one cycle may break several (they share the victim), and breaking one may leave another, so the detector removes the victim from the graph and searches again until no cycle is left.

## The alternatives

- **Prevention by ordering.** If every transaction takes locks in one global order, cycles cannot form. It works inside a program (lock A before B everywhere), not for a database, which does not know in advance what a query will touch.
- **Prevention by age (wait-die and wound-wait).** When T asks for a lock that U holds, the ages of T and U decide who waits and who is aborted, so a cycle never forms; some transactions are aborted that detection would have let live.
- **Timeouts.** Give up after a while. Simple, and slow: it waits out the timeout every time and aborts innocent transactions that were merely slow.

Detection costs a little CPU for the search, and saves aborting transactions that were never in a cycle. It is what PostgreSQL, MySQL InnoDB and SQL Server do.

## What can go wrong

- **Missing edges.** A request that is waiting for another *waiting* request (because the queue is fair) is also waiting on a transaction; a graph built from holders only misses cycles that go through the queue. Decide what an edge means and test it.
- **Acting on stale information.** The graph is a snapshot. By the time the victim is chosen, the lock may have been granted. Build the graph and choose victims under the same lock that guards grants.
- **Starving the same transaction.** A victim that retries immediately and is the youngest again can be aborted repeatedly. Real systems keep its original timestamp on retry.

## Invariants: what must always be true

- **An edge means a real wait.** T points to U exactly when T waits for a lock that U holds. The graph is rebuilt from the lock queues at each look, not kept by hand.
- **A deadlock is a cycle, and only a cycle.** A chain of waits ends at a transaction that is running and unwinds; a transaction that waits for nothing has no outgoing edge and cannot be in a cycle.
- **Every cycle gets exactly one victim,** and the choice is deterministic (the youngest transaction of the cycle). Two runs over the same graph abort the same transaction.
- **The victim's wait fails and its locks are released,** so the others in the cycle can go on. A victim that keeps its locks breaks nothing.
- **Detection repeats until no cycle is left.** Breaking one cycle can leave another that shared no victim with it.

## C++ comparison

| C / C++ | Rust |
|---|---|
| a detector thread with `std::thread` and an `atomic<bool>` stop flag | `thread::spawn`, an `Arc<AtomicBool>`, and `Drop` that sets the flag and joins |
| `std::map<txn_id_t, std::vector<txn_id_t>>` for the graph | `BTreeMap<TxnId, BTreeSet<TxnId>>`: ordered, so a search order is easy to fix |

## In real code

### Using it: a graph, a cycle, a victim

```rust test
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Graph {
    edges: BTreeMap<u32, BTreeSet<u32>>,
}

impl Graph {
    fn add(&mut self, from: u32, to: u32) {
        self.edges.entry(from).or_default().insert(to);
    }

    /// The youngest (largest id) transaction on a cycle, found by a depth-first search in id order.
    fn victim(&self) -> Option<u32> {
        fn visit(g: &Graph, n: u32, path: &mut Vec<u32>, done: &mut BTreeSet<u32>) -> Option<u32> {
            if let Some(at) = path.iter().position(|&p| p == n) {
                return path[at..].iter().copied().max();
            }
            if !done.insert(n) {
                return None;
            }
            path.push(n);
            for &next in g.edges.get(&n).into_iter().flatten() {
                if let Some(v) = visit(g, next, path, done) {
                    return Some(v);
                }
            }
            path.pop();
            None
        }
        let mut done = BTreeSet::new();
        self.edges.keys().find_map(|&s| visit(self, s, &mut Vec::new(), &mut done))
    }
}

#[test]
fn a_chain_is_not_a_deadlock() {
    let mut g = Graph::default();
    g.add(1, 2);
    g.add(2, 3);
    assert_eq!(g.victim(), None);
}

#[test]
fn a_cycle_loses_its_youngest_transaction() {
    let mut g = Graph::default();
    g.add(1, 2);
    g.add(2, 3);
    g.add(3, 1);
    g.add(9, 1); // waits for the cycle but is not part of it
    assert_eq!(g.victim(), Some(3));
}
```

### In the exercises

- **4d-06** builds `WaitsForGraph` (the structure above, with `remove_edge` and an ordered `edge_list`) and `LockManager::detect_deadlocks`, which turns the waiting requests into edges and aborts the victim of each cycle; `DeadlockDetector` runs it on a thread.
- **4d-07** has transactions that lock two accounts in random order, so deadlocks really happen, and checks that every one is resolved.

### Where it is used

PostgreSQL's `deadlock_timeout` triggers a detector that builds a waits-for graph from the lock tables; InnoDB runs a similar check when a lock wait starts; BusTub's lock manager project has the same background-thread design.
