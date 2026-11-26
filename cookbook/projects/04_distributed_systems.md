# 04. Distributed systems

Prerequisite track: `paths/tracks/07_distributed_systems.md`. Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet.

Test strategy for the whole theme: **simulate first, sockets later.** Build every protocol over a `Network` trait with an in-memory implementation that can drop, delay, duplicate and reorder messages from a seeded RNG and partition nodes. Print the seed on failure so any bug is replayable. Add real TCP only as a final milestone.

---

## 4.1 `resilient`: retry, backoff, hedging and circuit breaker  `S`

**Goal.** A client-side library wrapping any async operation with retry (exponential backoff + full jitter), per-attempt and total deadlines, a retry budget (at most 10% extra load), and a circuit breaker (closed / open / half-open).
**Learn.** Why naive retries cause retry storms, jitter, idempotency requirements, state machines with clocks, fake time.
**Read before.** `distributed/retry_backoff_jitter`, `distributed/token_bucket`. **During.** `web/http_client_resilience`, `errors/error_chain_retryable`, `patterns/state_machine`.
**Milestones.**
1. `retry(policy, op)`: base 50 ms, x2, cap 5 s, full jitter, max 6 attempts, only for errors classified retryable.
2. Total deadline 10 s overrides attempts; last error preserved in the chain.
3. Retry budget: token bucket refilled by 10% of successful calls.
4. Circuit breaker: open after 5 consecutive failures or 50% failure in a 20-call window; half-open probes 1 call after 30 s.
5. Hedged request: fire a second attempt after the p95 latency; cancel the loser.
**Acceptance.** Simulated server with 100 ms failure bursts and a fake clock: with budget on, total requests sent never exceed 1.1x offered load over a 10-minute simulation; breaker opens within 5 calls of a hard outage and closes within one probe interval after recovery.
**Data.** A fault-injecting service stub: error rate profile as a seeded piecewise function (0% / 100% / 30%).
**Write-up.** Which errors are safe to retry for a non-idempotent POST? What does the breaker do to latency percentiles?
**Stretch.** Adaptive concurrency limit (AIMD) driven by latency.
**Resembles.** [tower retry/limit layers](https://github.com/tower-rs/tower), Envoy outlier detection, [Envoy ratelimit](https://github.com/envoyproxy/ratelimit) for the server side.

## 4.2 `ringkv`: a consistent-hash sharded KV router  `M`

**Goal.** A router that maps keys to N storage nodes with consistent hashing (virtual nodes) and supports adding/removing a node with minimal key movement, plus replication to the next R-1 distinct nodes.
**Learn.** Ring structure, vnodes and balance, rendezvous hashing, rebalancing cost, replica placement across failure domains.
**Read before.** `distributed/consistent_hashing`, `distributed/hash_partitioning`. **During.** `data_structures/hash_functions`, `distributed/quorum_replication_dynamo`.
**Milestones.**
1. Ring with 128 vnodes/node, `lookup(key)` by binary search on sorted `(hash, node)`.
2. Balance report: max/mean load for 5-50 nodes over 1M keys (target under 1.25 at 128 vnodes).
3. Add 1 node to 10: moved keys within 10% +/- 1% of 1/11; remove similarly.
4. Replica set of R=3 on distinct nodes (and distinct "racks" tag if provided).
5. Compare against rendezvous hashing and jump hash on balance, movement, lookup cost.
**Acceptance.** Over 1M keys, node add moves only keys *to* the new node (never between old nodes); 100 random membership change sequences keep every key mapped; lookups under 200 ns median release.
**Data.** 1M keys of three styles: sequential ids, UUIDs, and Zipf-hot keys (to show hot-key limits of any hashing).
**Write-up.** What does vnode count trade off (memory, balance, rebalance granularity)? What do you do about one hot key?
**Stretch.** Bounded-load consistent hashing; weight-aware nodes.
**Resembles.** [Vitess](https://github.com/vitessio/vitess) sharding, Dynamo, the [Cassandra](https://github.com/apache/cassandra) token ring.

## 4.3 `minikafka`: a partitioned log with consumer groups  `M`

**Goal.** A broker with topics, partitions (append-only segmented logs with sparse offset index), producers (key -> partition), consumers with committed offsets, consumer groups with partition assignment and rebalance, and retention.
**Learn.** Log as the primitive, offset vs timestamp indexes, batching, at-least-once vs exactly-once (idempotent producer), rebalancing protocols, retention and compaction.
**Read before.** `distributed/kafka_log_model`, `distributed/record_batch_codec`, `os_and_storage/zero_copy_sendfile_splice`. **During.** `os_and_storage/wal_append_log`, `networking/tcp_framed_server`, `distributed/hash_partitioning`.
**Milestones.**
1. Single partition log: `append(batch) -> base_offset`, `fetch(offset, max_bytes)`, segments of 16 MiB with sparse index every 4 KiB.
2. Topic with P partitions; key-hash partitioner (same key -> same partition, ordering preserved per key).
3. TCP framed protocol (length-prefixed) for produce/fetch; fetch long-polls up to 500 ms.
4. Consumer group: coordinator assigns partitions evenly; heartbeat timeout 10 s triggers rebalance; offsets committed per (group, topic, partition).
5. Retention by size/time deletes whole segments; a consumer behind retention gets `OffsetOutOfRange`.
6. Idempotent producer: `(producer_id, sequence)` dedupes retried batches.
**Acceptance.** 1M messages produced by 4 producers, consumed by a 3-member group; kill one consumer mid-way: the union of consumed offsets has no gaps, duplicates only after the last committed offset; per-key order preserved; fetch throughput over 200 MB/s in-process.
**Data.** Messages `key=user_{zipf}`, 100 B payload with a global sequence number for gap checks.
**Write-up.** What does your group coordinator do on a split brain (member thinks it is alive but is not)? Where would exactly-once break?
**Stretch.** Log compaction by key; replication of a partition to 2 followers using the 4.5 Raft or a simple ISR scheme.
**Resembles.** [Apache Kafka](https://github.com/apache/kafka), [Redpanda](https://github.com/redpanda-data/redpanda), [NSQ](https://github.com/nsqio/nsq).

## 4.4 `lockd`: a lease-based lock service with fencing tokens  `M`

**Goal.** A single-server lock service granting leases with TTLs and monotonically increasing fencing tokens, and a protected resource that rejects stale tokens; then show the classic GC-pause bug and its fix.
**Learn.** Leases vs locks, clock skew, fencing, why a lock without fencing is unsafe, heartbeat/renew, expiry on the server's clock only.
**Read before.** `capstones/lease_fencing_sim`, `distributed/clock_sync_and_trace_context`. **During.** `paths/building/distributed_lock_leader.md`, `distributed/vector_clocks_hlc`, `patterns/state_machine`.
**Milestones.**
1. `acquire(name, ttl) -> (token, expires)`, `renew`, `release`; tokens strictly increasing per name.
2. Expiry driven by server fake clock; a client believing it holds the lock after expiry is the "zombie".
3. Protected storage rejects writes with token lower than the max seen.
4. Simulation: client A pauses 15 s after acquiring (TTL 10 s), B acquires, A resumes and writes: with fencing the write is rejected; without it data is corrupted (assert both outcomes).
5. Leader election recipe built on top (all candidates `acquire("leader")`; the holder runs the job).
**Acceptance.** Randomized schedule of 5 clients x 1,000 steps with random pauses and clock skew up to 20%: no two writes with decreasing token are ever accepted; at most one lease valid at any server instant.
**Data.** Seeded schedule generator emitting pause/resume/renew events.
**Write-up.** Why is "check the clock then act" never safe on the client? What would you need for the lock server itself to be fault-tolerant?
**Stretch.** Replace the single server with your Raft log (4.5); watch API for lock release.
**Resembles.** [etcd leases](https://github.com/etcd-io/etcd), Chubby (Burrows, 2006), Kleppmann's fencing post on martin.kleppmann.com.

## 4.5 `raftlab`: Raft leader election and log replication in a simulator  `L`

**Goal.** Implement Raft's core (terms, `RequestVote`, `AppendEntries`, commit index, state machine apply) as a deterministic, I/O-free state machine driven by a simulated network and clock, with a replicated KV on top.
**Learn.** Safety properties, election timeouts and randomization, log matching, why commit needs current-term entries, snapshots, determinism for testing.
**Read before.** `distributed/raft_consensus`, `distributed/paxos_basic_and_multi`, `patterns/sans_io_protocol`. **During.** `distributed/linearizability_checker`, `capstones/event_sourced_ledger`, `distributed/two_phase_commit`.
**Milestones.**
1. Election: 3 nodes elect exactly one leader per term; randomized timeout 150-300 ticks.
2. Log replication: leader appends, followers accept or reject with conflict hint, commit on majority.
3. Safety checker invoked after every simulation step: Election Safety, Log Matching, Leader Completeness, State Machine Safety.
4. Fault injection: random drops (10%), duplicates, reordering, partitions healing after 50-500 ticks, crash and restart with persistent term/vote/log.
5. Linearizable client ops (`put/get/cas`) with client request ids for dedupe; check history with the linearizability recipe.
6. Log compaction via snapshot + `InstallSnapshot`.
**Acceptance.** 1,000 seeds x 5,000 steps with 5 nodes: zero safety violations; liveness: after final heal, a leader emerges within 20 election timeouts in at least 99.9% of seeds; every committed value present on a majority at end.
**Data.** Seeds `0..1000`; a scenario DSL for named cases (leader isolated, split votes, stale leader returns).
**Write-up.** Why can a leader not commit entries from previous terms by counting replicas alone (Figure 8)? What do you persist before replying to RPCs?
**Stretch.** Membership change (joint consensus or single-server), pre-vote, read leases / ReadIndex.
**Resembles.** [etcd/raft](https://github.com/etcd-io/raft), [hashicorp/raft](https://github.com/hashicorp/raft), [TiKV raft-rs](https://github.com/tikv/tikv).

## 4.6 `dynamo`: a quorum KV with vector clocks and read repair  `M`

**Goal.** N=3 replicas per key, tunable R and W, sloppy quorum with hinted handoff, vector-clock versions with sibling resolution, read repair and anti-entropy via Merkle trees.
**Learn.** Quorum intersection (R+W>N), conflict detection vs resolution, causality tracking, hinted handoff, Merkle tree diffing.
**Read before.** `distributed/quorum_replication_dynamo`, `distributed/vector_clocks_hlc`, `cryptography/merkle_tree`. **During.** `distributed/consistent_hashing`, `distributed/crdts`.
**Milestones.**
1. Coordinator writes to W of N replicas and returns; reads R and returns the freshest by vector clock.
2. Concurrent writes produce siblings; client merges (provide LWW and set-union resolvers).
3. Read repair writes the merged value back to stale replicas.
4. Hinted handoff when a replica is down; hints replayed on recovery.
5. Merkle-tree anti-entropy between two replicas finds exactly the differing keys.
**Acceptance.** With R+W>N and no partitions, a read after a successful write always observes it (10,000 trials); with R=W=1 you can show a stale read; after a 5% random-drop run plus anti-entropy, all replicas converge to identical Merkle roots.
**Data.** Zipf keys 100k; fault schedule from the simulator in 4.5's style.
**Write-up.** Why do vector clocks grow and how do you prune them safely? When is LWW silently lossy?
**Stretch.** Dotted version vectors; tunable consistency per request.
**Resembles.** the Dynamo paper (DeCandia et al., 2007), [Apache Cassandra](https://github.com/apache/cassandra).

## 4.7 `swimmer`: gossip membership and failure detection  `L`

**Goal.** SWIM-style membership: periodic random probes, indirect probes via k=3 peers, suspicion with incarnation numbers, piggybacked gossip of membership updates, in a simulated network of 100 nodes.
**Learn.** Failure detector accuracy vs speed, epidemic dissemination (O(log n) rounds), incarnation/refutation, false positive analysis.
**Read before.** `distributed/swim_gossip`, `distributed/crdts`. **During.** `networking/udp_sockets_reliable`, `distributed/hash_partitioning`.
**Milestones.**
1. Ping/ack with timeout; mark `Suspect` after failed direct + indirect probes.
2. Suspect timeout (e.g. 5 x protocol period) then `Dead`; a live node refutes by bumping its incarnation.
3. Piggyback up to 6 updates per message, prioritised by fewest transmissions.
4. Simulate 100 nodes, crash 10: measure rounds until every live node knows (expect about log2(100)*c).
5. 5% packet loss: measure false-positive rate and tune the suspicion multiplier.
**Acceptance.** All 90 live nodes learn of all 10 crashes within 15 protocol periods in at least 95% of 200 seeded runs; false `Dead` declarations of live nodes under 0.1% at 5% loss.
**Data.** Seeds 0..200, loss rates {0,1,5,10}%.
**Write-up.** Why suspicion instead of immediate death? What is the bandwidth per node per period?
**Stretch.** Real UDP implementation with 5 processes on localhost; Lifeguard-style local health awareness.
**Resembles.** [HashiCorp memberlist](https://github.com/hashicorp/memberlist) (SWIM implementation), [Cassandra gossip](https://github.com/apache/cassandra).

---

## Katas (30 minutes each)

1. Implement a Lamport clock; write a test with 3 processes proving `a -> b` implies `L(a) < L(b)` and give a counterexample for the converse.
2. Write `merge(a: VectorClock, b: VectorClock)` and `compare` returning `Before/After/Equal/Concurrent`; exhaustively check 3 nodes with counters 0..3.
3. Build a G-Counter and PN-Counter CRDT; property-test commutativity, associativity, idempotence of `merge` on 1,000 random states.
4. Simulate 1,000 keys on 10 nodes with plain `hash % N`, then `N=11`: measure the fraction that moves (expect about 91%) vs consistent hashing (about 9%).
5. Write full-jitter backoff and print the first 6 delays for base 100 ms cap 10 s with a fixed seed; assert each lies in `[0, min(cap, base*2^n)]`.
6. Implement a token bucket (rate 100/s, burst 20) with a fake clock; assert at most `burst + rate*t` admissions over any window.
7. Simulate a two-phase commit with a coordinator crash after PREPARE and show participants blocked; document what a termination protocol would need.
8. Compute quorum overlap by brute force: for N=5 list all (R, W) with R+W>N and confirm every read set intersects every write set.
9. Implement an LWW-Register with HLC timestamps; show a case where wall-clock skew of 2 s lets an old write win with plain timestamps but not with HLC.
10. Write a Merkle root over 1,024 leaves; change 1 leaf and report the number of nodes to re-hash (expect log2 + 1 = 11).
