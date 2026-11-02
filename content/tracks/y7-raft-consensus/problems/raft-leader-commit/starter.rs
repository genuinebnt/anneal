/// The leader's commit rule. `log_terms[i]` is the term of entry `i + 1` in the leader's log; `follower_match[j]` is the highest
/// index known to be replicated on follower `j` (the leader itself holds its whole log). Returns the new commit index.
pub fn advance_commit(log_terms: &[u64], current_term: u64, follower_match: &[u64], commit_index: u64) -> u64 {
    todo!()
}
