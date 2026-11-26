# Chapter `distributed`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`retry_backoff_jitter`](../../examples/distributed/retry_backoff_jitter.rs) | simple | exponential backoff with full jitter. |
| 2 | [`clock_sync_and_trace_context`](../../examples/distributed/clock_sync_and_trace_context.rs) | intermediate | clock synchronisation (Cristian, NTP, Berkeley, slew vs step) and W3C Trace Context |
| 3 | [`consistent_hashing`](../../examples/distributed/consistent_hashing.rs) | intermediate | consistent hashing ring with virtual nodes. |
| 4 | [`hash_partitioning`](../../examples/distributed/hash_partitioning.rs) | intermediate | partitioning keys across N nodes: modulo, consistent-hash ring, rendezvous (HRW), Jump Consistent Hash, Maglev table. |
| 5 | [`record_batch_codec`](../../examples/distributed/record_batch_codec.rs) | intermediate | a Kafka-style record batch codec: zigzag varints, CRC32C, delta-encoded records, zero-copy decode. |
| 6 | [`token_bucket`](../../examples/distributed/token_bucket.rs) | intermediate | token-bucket rate limiter with an injectable clock. |
| 7 | [`vector_clocks_hlc`](../../examples/distributed/vector_clocks_hlc.rs) | intermediate | Lamport clocks, vector clocks / version vectors, hybrid logical clocks and causal delivery. |
| 8 | [`chandy_lamport_snapshot`](../../examples/distributed/chandy_lamport_snapshot.rs) | advanced | Chandy-Lamport distributed snapshot (consistent cuts) on FIFO channels |
| 9 | [`crdts`](../../examples/distributed/crdts.rs) | advanced | state-based CRDTs (counters, registers, sets, map) plus an RGA sequence CRDT, verified by law tests. |
| 10 | [`kafka_log_model`](../../examples/distributed/kafka_log_model.rs) | advanced | a model of Kafka's core: partitioned segmented logs, retention, compaction, idempotent producer, consumer groups, replication. |
| 11 | [`linearizability_checker`](../../examples/distributed/linearizability_checker.rs) | advanced | linearizability checker (Wing-Gong search with memoisation) vs sequential models |
| 12 | [`operational_transform`](../../examples/distributed/operational_transform.rs) | advanced | operational transformation (OT) for plain text, with a central-server client/server model. |
| 13 | [`paxos_basic_and_multi`](../../examples/distributed/paxos_basic_and_multi.rs) | advanced | Paxos: single-decree safety under chaos, dueling-proposer livelock, and Multi-Paxos replicated log |
| 14 | [`quorum_replication_dynamo`](../../examples/distributed/quorum_replication_dynamo.rs) | advanced | Dynamo-style quorum replication, hinted handoff, read repair, version vectors |
| 15 | [`raft_consensus`](../../examples/distributed/raft_consensus.rs) | advanced | Raft consensus in a deterministic simulation: elections, log replication, repair, crashes and partitions |
| 16 | [`saga_orchestrator`](../../examples/distributed/saga_orchestrator.rs) | advanced | the saga pattern: distributed transactions without 2PC, via compensation, a durable log and idempotent steps |
| 17 | [`spanner_truetime_commit_wait`](../../examples/distributed/spanner_truetime_commit_wait.rs) | advanced | Spanner external consistency: TrueTime intervals, commit wait, 2PL + 2PC over replicated groups, snapshot reads, safe time. |
| 18 | [`swim_gossip`](../../examples/distributed/swim_gossip.rs) | advanced | SWIM failure detection and gossip membership, simulated. |
| 19 | [`two_phase_commit`](../../examples/distributed/two_phase_commit.rs) | advanced | atomic commit: two-phase commit with write-ahead logs, crash recovery, blocking, presumed abort/commit, and 3PC |
