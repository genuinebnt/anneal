# Distributed Systems

Consistent hashing, CRDTs, OT, clocks, SWIM, Kafka-style logs, leases and fencing, sagas, retries, rate limiting.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

19 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`retry_backoff_jitter`](retry_backoff_jitter.rs) | exponential backoff with full jitter | simple | every network client that retries (HTTP, RPC, queue consumers): jitter prevents synchronized retry storms. | std only |
| [`token_bucket`](token_bucket.rs) | token-bucket rate limiter with an injectable clock | intermediate | API gateways, per-tenant quotas, login throttling, outbound call budgets. | std only |
| [`consistent_hashing`](consistent_hashing.rs) | consistent hashing ring with virtual nodes | intermediate | sharding keys across caches/DB nodes (Cassandra, memcached clients) so adding a node moves ~1/N of keys. | std only |
| [`hash_partitioning`](hash_partitioning.rs) | partitioning keys across N nodes: modulo, consistent-hash ring, rendezvous (HRW), Jump Consistent Hash, Maglev table | intermediate | cache-cluster sharding (client-side hashing in memcached-style clients), Dynamo-style stores using ring partitioning, load balancers | std only |
| [`crdts`](crdts.rs) | state-based CRDTs (counters, registers, sets, map) plus an RGA sequence CRDT, verified by law tests | advanced | collaborative editors and note apps, offline-first sync, shopping carts / presence / like counters in | std only |
| [`operational_transform`](operational_transform.rs) | operational transformation (OT) for plain text, with a central-server client/server model | advanced | server-mediated collaborative editors (Google Docs is widely documented as OT-based), shared notes, chat | std only |
| [`vector_clocks_hlc`](vector_clocks_hlc.rs) | Lamport clocks, vector clocks / version vectors, hybrid logical clocks and causal delivery | intermediate | Dynamo-style stores keep siblings for concurrent writes and let the application merge them (Riak documents this), | std only |
| [`swim_gossip`](swim_gossip.rs) | SWIM failure detection and gossip membership, simulated | advanced | cluster membership and failure detection in HashiCorp memberlist (used by Serf and | std only |
| [`kafka_log_model`](kafka_log_model.rs) | a model of Kafka's core: partitioned segmented logs, retention, compaction, idempotent producer, consumer groups, replication | advanced | event sourcing and CDC pipelines, durable work queues, metrics/log aggregation, stream processing state | std only |
| [`record_batch_codec`](record_batch_codec.rs) | a Kafka-style record batch codec: zigzag varints, CRC32C, delta-encoded records, zero-copy decode | intermediate | Kafka-compatible brokers and clients batch records exactly like this to amortize per-message overhead; | std only |
| [`saga_orchestrator`](saga_orchestrator.rs) | the saga pattern: distributed transactions without 2PC, via compensation, a durable log and idempotent steps | advanced | e-commerce checkout (inventory, payment, shipping), travel booking, order fulfilment, account provisioning. | std only |
| [`raft_consensus`](raft_consensus.rs) | Raft consensus in a deterministic simulation: elections, log replication, repair, crashes and partitions | advanced | etcd and the Kubernetes control plane, Consul, CockroachDB and TiKV (a Raft group per range), RethinkDB, | std only |
| [`paxos_basic_and_multi`](paxos_basic_and_multi.rs) | Paxos: single-decree safety under chaos, dueling-proposer livelock, and Multi-Paxos replicated log | advanced | Google Chubby and Spanner (Multi-Paxos per tablet), Apache Cassandra lightweight transactions (single-decree | std only |
| [`two_phase_commit`](two_phase_commit.rs) | atomic commit: two-phase commit with write-ahead logs, crash recovery, blocking, presumed abort/commit, and 3PC | advanced | XA transactions (Java JTA, PostgreSQL PREPARE TRANSACTION, MySQL XA), Microsoft DTC, Google Spanner | std only |
| [`quorum_replication_dynamo`](quorum_replication_dynamo.rs) | Dynamo-style quorum replication, hinted handoff, read repair, version vectors | advanced | tunable consistency in Cassandra (QUORUM / ONE / ALL consistency levels), Riak | std only |
| [`chandy_lamport_snapshot`](chandy_lamport_snapshot.rs) | Chandy-Lamport distributed snapshot (consistent cuts) on FIFO channels | advanced | Apache Flink's checkpoints are Chandy-Lamport with barriers (aligned barriers = markers on | std only |
| [`linearizability_checker`](linearizability_checker.rs) | linearizability checker (Wing-Gong search with memoisation) vs sequential models | advanced | Jepsen's Knossos checker (Clojure) and the Go library porcupine run exactly this search over | std only |
| [`clock_sync_and_trace_context`](clock_sync_and_trace_context.rs) | clock synchronisation (Cristian, NTP, Berkeley, slew vs step) and W3C Trace Context | intermediate | OpenTelemetry, Jaeger and Zipkin propagate exactly this header (W3C Trace Context); distributed | std only |
| [`spanner_truetime_commit_wait`](spanner_truetime_commit_wait.rs) | Spanner external consistency: TrueTime intervals, commit wait, 2PL + 2PC over replicated groups, snapshot reads, safe time | advanced | Google Spanner (TrueTime from GPS and atomic clocks), CockroachDB (hybrid logical clocks with an uncertainty interval, optional commit wait), | std only |
