# Building: Leader election, locks and coordination



Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Why it is hard

**Goal:** Clocks and pauses.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`vector_clocks_hlc`](../../examples/distributed/vector_clocks_hlc.rs) · intermediate | Time. |
| 1.2 | [`lease_fencing_sim`](../../examples/capstones/lease_fencing_sim.rs) · capstone | Fencing tokens. |
| 1.3 | [`linearizability_checker`](../../examples/distributed/linearizability_checker.rs) · advanced | What 'correct' means. |

### Step 2: Consensus

**Goal:** Build on it, don't reinvent.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`raft_consensus`](../../examples/distributed/raft_consensus.rs) · advanced | Raft. |
| 2.2 | [`paxos_basic_and_multi`](../../examples/distributed/paxos_basic_and_multi.rs) · advanced | Paxos. |
| 2.3 | [`swim_gossip`](../../examples/distributed/swim_gossip.rs) · advanced | Membership. |
