BusTub's `ORSetDriver` tests: a driver holds several replicas; each can `save` its state to a shared place and `load` the copies the others saved; `sync` has everyone save and then everyone load. The tests add, remove, sync and check that replicas agree, including a case where two replicas exchange states and a third is cut off.

## The task

The driver (`ORSetDriver`) is given; it only needs your `ORSet`. Make the stage's tests pass: `cargo test --test stages_0d s0d_08`.

The tests: BusTub's driver tests, ported (add, remove and sync across three nodes; two nodes agree after a sync; removing everything and adding it back; adds win a lot; a lost network still converges later) and the network property of stage 4.

## Your freedom

None new.

## The Rust toolbox

**The driver.** `ORSetDriver` keeps a replica per node and a 'published' copy; `save(n)` publishes, `load(n)` merges the copies other nodes published since it last read, `sync()` saves every node and then loads every node. Dropping a `save` or a `load` simulates a lost message.

**Reading a failure.** The test prints the state of each replica; the first element on which two replicas disagree after a full sync is the one whose add or remove your `merge` or `remove` mishandled.

## Design notes

**How the driver works.** `save(node)` copies the node's state to the shared slot and bumps a version; `load(node)` merges every other node's saved copy that is newer than the last one it read. Because merging is idempotent, re-reading is harmless, and because it is commutative and associative the order in which nodes load does not matter.

**Failures are modelled by skipping steps.** A lost network is a `save` or `load` that is simply not called. The set converges whenever the missing steps happen later: that is the CRDT guarantee.

**If a test fails.** Compare the two replicas' `to_string()`: a missing element means a remove was applied to a pair it had not seen (check `remove`); an element that should be gone means `merge` dropped a removed pair.

## If this is new

- Everything is in the earlier stages of this module.
- Y5 Testing & verification: Understand it: algebraic laws (commutative, associative, idempotent) as properties.

## Tests

- BusTub's driver tests; the network convergence property.

## Hints

### Print the sets

`d.set(i).to_string()` for each node shows immediately which replica disagrees.

### Idempotence

If a second `sync` changes anything, `merge` is not a pure union.

## Performance

The tests run a handful of replicas and a few dozen operations each: milliseconds.

## Experiment

Optional. Predict first, then run.

1. **Merge only the adds.** Which scenario fails?
2. **Syncing once instead of twice.** In the property, is one full sync enough? What does that say about the driver?

## Other designs

None for this stage. The *Other designs* sections of 0d-01 to 0d-04 list the alternatives to compare with yours.

## In BusTub

`test/primer/orset_test.cpp`: `ORSetDriverTest.AddRemoveTest`, `MergeTest`, `AddBackAgainTest`, `AddWinsALotTest`, `NetworkLostTest`; the driver is `orset_driver.cpp`: "`Saves changes in all nodes and then load all the changes.`".

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `driver[i % 3]->Add(i)` | `driver.add(i % 3, &i)` |
| `ORSetNode` holding a pointer back to its driver | the driver owns the replicas; nodes are indexes |

**Port rule:** objects that need their owner's services become indexes into the owner.

## Learn more

- [Jepsen: consistency models](https://jepsen.io/consistency) · [Convergence in CRDTs](https://crdt.tech/)
