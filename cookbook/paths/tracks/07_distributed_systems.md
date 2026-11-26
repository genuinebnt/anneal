# Track: Distributed systems

About 6 weeks.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Time and ordering

**Goal:** No global clock.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`vector_clocks_hlc`](../../examples/distributed/vector_clocks_hlc.rs) · intermediate | Logical and hybrid clocks. |
| 1.2 | [`clock_sync_and_trace_context`](../../examples/distributed/clock_sync_and_trace_context.rs) · intermediate | Clocks and tracing. |
| 1.3 | [`chandy_lamport_snapshot`](../../examples/distributed/chandy_lamport_snapshot.rs) · advanced | Consistent snapshots. |

### Step 2: Partitioning and replication

**Goal:** Spread and copy data.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`consistent_hashing`](../../examples/distributed/consistent_hashing.rs) · intermediate | Placement. |
| 2.2 | [`hash_partitioning`](../../examples/distributed/hash_partitioning.rs) · intermediate | Partition schemes. |
| 2.3 | [`quorum_replication_dynamo`](../../examples/distributed/quorum_replication_dynamo.rs) · advanced | Quorums. |
| 2.4 | [`crdts`](../../examples/distributed/crdts.rs) · advanced | Merge without coordination. |
| 2.5 | [`operational_transform`](../../examples/distributed/operational_transform.rs) · advanced | Collaborative editing. |

### Step 3: Consensus and transactions

**Goal:** Agree despite failure.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`paxos_basic_and_multi`](../../examples/distributed/paxos_basic_and_multi.rs) · advanced | Paxos. |
| 3.2 | [`raft_consensus`](../../examples/distributed/raft_consensus.rs) · advanced | Raft. |
| 3.3 | [`two_phase_commit`](../../examples/distributed/two_phase_commit.rs) · advanced | 2PC. |
| 3.4 | [`saga_orchestrator`](../../examples/distributed/saga_orchestrator.rs) · advanced | Sagas. |
| 3.5 | [`linearizability_checker`](../../examples/distributed/linearizability_checker.rs) · advanced | Check your claims. |
| 3.6 | [`swim_gossip`](../../examples/distributed/swim_gossip.rs) · advanced | Failure detection. |

### Step 4: Messaging and resilience

**Goal:** What services actually use.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`kafka_log_model`](../../examples/distributed/kafka_log_model.rs) · advanced | Log-structured messaging. |
| 4.2 | [`record_batch_codec`](../../examples/distributed/record_batch_codec.rs) · intermediate | Batch formats. |
| 4.3 | [`retry_backoff_jitter`](../../examples/distributed/retry_backoff_jitter.rs) · simple | Retries. |
| 4.4 | [`token_bucket`](../../examples/distributed/token_bucket.rs) · intermediate | Rate limiting. |
| 4.5 | [`lease_fencing_sim`](../../examples/capstones/lease_fencing_sim.rs) · capstone | Leases and fencing (FlowForge). |
| 4.6 | [`event_sourced_ledger`](../../examples/capstones/event_sourced_ledger.rs) · capstone | Event sourcing. |
