# Practice: distributed

How to use this page: run the recipe (`cargo run --release --example <name>`), answer Prove you got it, then do the Try next tasks in order.
Each task names something to check: an oracle, an invariant, a fault you inject, or a number you measure. Write the check before the change.
No solutions here. S about 30 min, M about 2 h, A about a day. Recipe files are in `examples/distributed/`.

## chandy_lamport_snapshot
**Prove you got it**
1. Why must a process record the state of an incoming channel only between its own snapshot and the marker arriving on that channel?
2. What invariant over the whole snapshot (local states plus in-flight messages) do the money tokens give you?

**Try next**
- [S] Take the snapshot at 50 different random moments and assert total money in the snapshot equals the initial total every time.
- [M] Break FIFO (reorder messages on one channel) and show the snapshot becomes inconsistent; then say what the algorithm would need.
- [A] Re-implement it on real threads with `mpsc` channels as FIFO links and verify the same conservation invariant under random scheduling.

**Use it in:** consistent checkpoints of a streaming job; Apache Flink's barrier snapshots are this algorithm ([apache/flink](https://github.com/apache/flink)).
**Pitfall:** markers on a non-FIFO channel give a cut that was never a real state.

## clock_sync_and_trace_context
**Prove you got it**
1. Prove Cristian's error bound from the code: where does `(d_req - d_resp)/2` come from?
2. In a `traceparent` header, which fields must a service keep and which does it regenerate?

**Try next**
- [S] Run Cristian with asymmetric delays and plot the real error against the bound.
- [M] Add slew-versus-step clock correction and assert the clock never goes backwards under slew.
- [A] Propagate a trace context across three simulated services (HTTP headers via `http_client_server_loopback` in the web chapter) and assert every span has the right parent.

**Use it in:** NTP clients, distributed tracing. Real code: [open-telemetry/opentelemetry-rust](https://github.com/open-telemetry/opentelemetry-rust).
**Pitfall:** using wall-clock time for durations; use a monotonic clock.

## consistent_hashing
**Prove you got it**
1. About what fraction of keys move when an 11th node joins, and why is `hash % N` worse?
2. What does the number of virtual points per node trade off?

**Try next**
- [S] Measure max/mean load for 1, 10, 100, 1000 vnodes and pick a knee.
- [M] Add weighted nodes (capacity 1x and 3x) and assert load tracks weight within 10%.
- [A] Replicate each key to the next 3 distinct nodes and assert no key loses all replicas when any 2 nodes are removed.

**Use it in:** caches and sharded stores. Real code: [hashicorp/memberlist](https://github.com/hashicorp/memberlist) shows membership around such rings; Cassandra uses vnodes ([apache/cassandra](https://github.com/apache/cassandra)).
**Pitfall:** a poor hash gives clustered vnodes and uneven arcs.

## crdts
**Prove you got it**
1. State the three merge laws and give a counter-example type that violates one (a plain counter with `+`).
2. Why does an OR-Set need unique tags per add?

**Try next**
- [S] Add a property test that merges 3 replicas in all 6 orders and with duplicates, asserting one result.
- [M] Add a causal-context delta for the set so replicas ship only changes; assert delta-merge equals full-state merge.
- [A] Build a replicated shopping cart across 3 simulated replicas with random partitions and compare with `operational_transform` on the same edit trace for convergence and intent preservation.

**Use it in:** collaborative editing and offline-first apps. Real code: [automerge/automerge](https://github.com/automerge/automerge), [y-crdt/y-crdt](https://github.com/y-crdt/y-crdt).
**Pitfall:** tombstones grow forever without a causal-stability GC.

## hash_partitioning
**Prove you got it**
1. Which of the five schemes needs a lookup table and what does it buy?
2. Why does Jump hash not support removing an arbitrary node?

**Try next**
- [S] Print movement fraction when adding a node for each scheme and compare to 1/(N+1).
- [M] Add a skewed key distribution (Zipf) and report max load per scheme.
- [A] Build a router that supports node removal on top of Jump (hint: bucket-to-node indirection) and test movement.

**Use it in:** shard routing. Real code: Envoy's Maglev LB ([envoyproxy/envoy](https://github.com/envoyproxy/envoy)).
**Pitfall:** changing the hash function version silently remaps everything.

## kafka_log_model
**Prove you got it**
1. What ordering does Kafka guarantee and where does it stop?
2. How does the idempotent producer reject a duplicate?

**Try next**
- [S] Assert a consumer group with 3 consumers and 6 partitions assigns 2 each, and reassigns after one leaves.
- [M] Add log compaction and assert the latest value per key is preserved while older ones go.
- [A] Persist segments to disk using the `record_batch_codec` format and recover after truncation of the last segment.

**Use it in:** event pipelines. Real code: [apache/kafka](https://github.com/apache/kafka).
**Pitfall:** committing offsets before processing gives at-most-once.

## linearizability_checker
**Prove you got it**
1. Write a 3-operation history that is sequentially consistent but not linearizable.
2. Why does memoisation of (state, set of done ops) prune the search?

**Try next**
- [S] Add a queue model test with one illegal history.
- [M] Run the checker against a deliberately buggy replicated register (stale read) from your own simulation and see it fail.
- [A] Feed it histories recorded from a real multi-threaded `Mutex<HashMap>` and from a deliberately racy one.

**Use it in:** testing databases. Real code: [anishathalye/porcupine](https://github.com/anishathalye/porcupine), [jepsen-io/jepsen](https://github.com/jepsen-io/jepsen).
**Pitfall:** timeouts: an unknown-outcome operation must be treated as possibly applied.

## operational_transform
**Prove you got it**
1. State TP1 and check one insert/delete pair by hand.
2. Why does the server serialise operations?

**Try next**
- [S] Fuzz two clients with 100 random ops each and assert convergence.
- [M] Add a third client and note which property (TP2) the central server lets you avoid.
- [A] Compare convergence behaviour with the RGA in `crdts` on the same trace.

**Use it in:** Google Docs-style editors. Real code: [share/sharedb](https://github.com/share/sharedb) and [y-crdt/y-crdt](https://github.com/y-crdt/y-crdt) for the CRDT alternative.
**Pitfall:** tie-breaking for two inserts at the same index must be deterministic.

## paxos_basic_and_multi
**Prove you got it**
1. Why must a proposer adopt the highest-numbered accepted value from promises?
2. What causes dueling proposers and how does a stable leader fix it?

**Try next**
- [S] Run 1000 seeds of single-decree and assert at most one value ever chosen.
- [M] Add acceptor crash and restart with persistent state and assert safety still holds.
- [A] Compare with `raft_consensus` on the same fault schedule: count messages and rounds.

**Use it in:** replicated state machines. Real code: [databendlabs/openraft](https://github.com/databendlabs/openraft) for the Raft analogue.
**Pitfall:** forgetting to persist `promised` before replying.

## quorum_replication_dynamo
**Prove you got it**
1. Why does R + W > N guarantee reading your write, and what breaks with sloppy quorums?
2. What does read repair fix and what does it not?

**Try next**
- [S] Vary (N,R,W) over 4 settings and count stale reads under 20% message loss.
- [M] Add concurrent writes to one key and show version vectors produce siblings.
- [A] Add Merkle-tree anti-entropy (see `merkle_tree` in cryptography) between two replicas and measure bytes exchanged versus a full sync.

**Use it in:** Cassandra/Riak style stores. Real code: [apache/cassandra](https://github.com/apache/cassandra).
**Pitfall:** hinted handoff can deliver very old writes after long outages.

## raft_consensus
**Prove you got it**
1. Why can a leader only commit entries of its own term by counting replicas?
2. What state must be persistent and why?

**Try next**
- [S] Run 200 seeds and assert the Log Matching and State Machine Safety properties.
- [M] Add log compaction with a snapshot and an `InstallSnapshot` to a lagging follower.
- [A] Add membership change (joint consensus or single-server) and test adding and removing a node under partition.

**Use it in:** etcd, Consul, TiKV. Real code: [etcd-io/raft](https://github.com/etcd-io/raft), [tikv/raft-rs](https://github.com/tikv/raft-rs), [databendlabs/openraft](https://github.com/databendlabs/openraft).
**Pitfall:** reading from a deposed leader returns stale data without a read index or lease.

## record_batch_codec
**Prove you got it**
1. Why is `base_offset` not covered by the CRC?
2. How does `batch_length` let a reader skip unknown batches?

**Try next**
- [S] Flip every single bit of an encoded batch and assert decode rejects all of them.
- [M] Add a compression attribute (reuse `compression_lz77_huffman`) and keep the CRC over the compressed bytes.
- [A] Write a fuzz target that never panics and never allocates more than the input size times a constant.

**Use it in:** log and message formats. Real code: [apache/kafka](https://github.com/apache/kafka) record batch.
**Pitfall:** trusting an element count from the header before checking remaining bytes.

## retry_backoff_jitter
**Prove you got it**
1. What does full jitter do to the synchronisation of retries?
2. What are the caps and why?

**Try next**
- [S] Simulate 1000 clients and plot retry arrivals per 10 ms with and without jitter.
- [M] Add a retry budget (max 10% extra load) and a deadline; assert it gives up.
- [A] Combine with `token_bucket` in a client-side limiter and prove a server under overload recovers faster.

**Use it in:** any RPC client. Real code: [tower-rs/tower](https://github.com/tower-rs/tower) retry.
**Pitfall:** retrying non-idempotent calls.

## saga_orchestrator
**Prove you got it**
1. Why must compensations be idempotent?
2. What does the durable log buy after an orchestrator crash?

**Try next**
- [S] Inject a failure at each step and assert all prior steps are compensated in reverse order.
- [M] Crash the orchestrator at every log position and assert recovery completes or compensates.
- [A] Contrast with `two_phase_commit` for the same scenario: count blocked locks and time to resolution.

**Use it in:** order/payment flows. Real code: [temporalio/temporal](https://github.com/temporalio/temporal).
**Pitfall:** a compensation that itself can fail without a retry policy.

## spanner_truetime_commit_wait
**Prove you got it**
1. What is the commit-wait duration in terms of the TrueTime interval?
2. What are safe-time and why do snapshot reads wait?

**Try next**
- [S] Increase clock uncertainty and measure commit latency growth.
- [M] Inject a clock error larger than the bound and show external consistency can break.
- [A] Compare with `vector_clocks_hlc`'s HLC approach on the same trace of transactions.

**Use it in:** globally consistent DBs. Real code: [cockroachdb/cockroach](https://github.com/cockroachdb/cockroach) uses HLC plus uncertainty intervals.
**Pitfall:** assuming uncertainty bounds hold when a VM pauses.

## swim_gossip
**Prove you got it**
1. How does an indirect probe reduce false positives?
2. Why does suspicion carry an incarnation number?

**Try next**
- [S] Measure detection time for 20, 100, 500 nodes.
- [M] Add a slow node (not dead) and measure false-positive rate with and without suspicion.
- [A] Compare the dissemination with `crdts` gossip for membership state.

**Use it in:** service discovery. Real code: [hashicorp/memberlist](https://github.com/hashicorp/memberlist), [hashicorp/serf](https://github.com/hashicorp/serf).
**Pitfall:** a refuted-suspicion needs a higher incarnation or the node stays dead.

## token_bucket
**Prove you got it**
1. How does capacity vs refill rate control burst vs long-run rate?
2. Why is the clock injected?

**Try next**
- [S] Assert over 10 simulated seconds the admitted count is at most `capacity + rate*10`.
- [M] Add per-key buckets with idle eviction and a memory bound.
- [A] Compare with the six algorithms in `rate_limiter_lab` (capstones).

**Use it in:** API gateways. Real code: [boinkor-net/governor](https://github.com/boinkor-net/governor).
**Pitfall:** refilling with floating-point drift.

## two_phase_commit
**Prove you got it**
1. Where is the commit point and why?
2. What blocks if the coordinator dies after prepare?

**Try next**
- [S] Crash each participant at each step and assert atomicity.
- [M] Add presumed abort and count log writes saved.
- [A] Add a cooperative termination protocol and show when blocking remains.

**Use it in:** XA transactions. Real code: [postgres/postgres](https://github.com/postgres/postgres) `PREPARE TRANSACTION`.
**Pitfall:** a participant that times out after voting yes must not abort alone.

## vector_clocks_hlc
**Prove you got it**
1. Why can Lamport clocks not detect concurrency?
2. How does HLC stay close to physical time?

**Try next**
- [S] Compare two events and print Before/After/Concurrent for 5 cases.
- [M] Add causal delivery buffering and assert delivery order respects causality under random delay.
- [A] Use HLC stamps in the `quorum_replication_dynamo` conflict resolution and compare to version vectors.

**Use it in:** conflict resolution. Real code: [cockroachdb/cockroach](https://github.com/cockroachdb/cockroach) HLC.
**Pitfall:** version vector growth with many writers.

## Chapter projects

Other-chapter stems used: `merkle_tree`, `hmac_hkdf_pbkdf2` (cryptography), `axum_router_state` (web), `mini_kv_store`, `lease_fencing_sim`, `rate_limiter_lab` (capstones), `bloom_filter` (search_vectors).

### Small (about 2 h): idempotent retrying client
Combine `retry_backoff_jitter`, `token_bucket`, `axum_idempotency` (web).
Milestones: (1) flaky axum endpoint; (2) client with backoff, jitter and a budget; (3) idempotency keys.
Acceptance: under 30% failure injection, every request is applied exactly once and total retries stay under the budget.
Stretch: add a circuit breaker.

### Intermediate (a weekend): replicated KV with anti-entropy
Combine `quorum_replication_dynamo`, `consistent_hashing`, `vector_clocks_hlc`, `merkle_tree`.
Milestones: (1) 5 nodes on a ring; (2) N/R/W with version vectors; (3) hinted handoff; (4) Merkle anti-entropy.
Acceptance: after a random partition schedule, replicas converge, and a linearizability-style check shows quorum reads see the latest acknowledged write when R+W>N.
Stretch: use `linearizability_checker` as the oracle.

### Advanced (a week): Raft-backed key-value service
Combine `raft_consensus`, `linearizability_checker`, `swim_gossip`, `mini_kv_store`, `axum_router_state`.
Milestones: (1) Raft log persisted in `mini_kv_store`; (2) HTTP API; (3) snapshots; (4) fault-injection harness that crashes and partitions; (5) record histories and check them.
Acceptance: 1000 seeds with random faults yield histories your checker accepts; the committed prefix is identical on all live nodes.
Stretch: lease reads (see `lease_fencing_sim`) with measured latency win.
