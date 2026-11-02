Implement the follower side of Raft's log replication: `Node::handle_append` (the `AppendEntries` RPC; a heartbeat is
one with no entries). Log indexes are **1-based**: `log[0]` is entry 1.

1. A request from an older term is rejected, replying with our term. A newer term is adopted (clearing `voted_for`).
2. **Consistency check:** if `prev_log_index > 0`, we must hold an entry at that index whose term is `prev_log_term`;
   otherwise reply `success: false` and change nothing else. `prev_log_index == 0` always passes.
3. Walk the new entries. If an entry's index already exists in our log with a **different** term, delete that entry and
   everything after it, then append. If it exists with the **same** term, keep ours (do not truncate: the request may be
   a duplicate or reordered older message). Entries past the end are appended.
4. If `leader_commit > commit_index`, set `commit_index = min(leader_commit, index of the last entry this request
   covered)` (that is `prev_log_index + entries.len()`), but never let it decrease.
5. On success reply `match_index = prev_log_index + entries.len()`.
