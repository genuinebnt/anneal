# Building: A message queue / Kafka-like log



Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: The log

**Goal:** Append-only storage.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`wal_append_log`](../../examples/os_and_storage/wal_append_log.rs) · advanced | Segments. |
| 1.2 | [`kafka_log_model`](../../examples/distributed/kafka_log_model.rs) · advanced | Partitions, offsets, consumer groups. |
| 1.3 | [`record_batch_codec`](../../examples/distributed/record_batch_codec.rs) · intermediate | Batch format. |
| 1.4 | [`encodings_codecs`](../../examples/encoding_compression/encodings_codecs.rs) · intermediate | Varints. |

### Step 2: Delivery

**Goal:** Semantics.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`retry_backoff_jitter`](../../examples/distributed/retry_backoff_jitter.rs) · simple | Retries. |
| 2.2 | [`saga_orchestrator`](../../examples/distributed/saga_orchestrator.rs) · advanced | Exactly-once illusions. |
| 2.3 | [`bounded_channel`](../../examples/async/bounded_channel.rs) · intermediate | Backpressure. |

### Step 3: Cluster

**Goal:** Replicate.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`raft_consensus`](../../examples/distributed/raft_consensus.rs) · advanced | Replicated log. |
| 3.2 | [`quorum_replication_dynamo`](../../examples/distributed/quorum_replication_dynamo.rs) · advanced | Quorums. |
| 3.3 | [`consistent_hashing`](../../examples/distributed/consistent_hashing.rs) · intermediate | Partition placement. |
