pub fn advance_commit(log_terms: &[u64], current_term: u64, follower_match: &[u64], commit_index: u64) -> u64 {
    let cluster = follower_match.len() + 1; // the followers and the leader
    let majority = cluster / 2 + 1;
    let mut best = commit_index;
    for idx in (commit_index + 1)..=(log_terms.len() as u64) {
        // only an entry of the leader's own term is committed by counting replicas (the Figure 8 rule)
        let replicas = 1 + follower_match.iter().filter(|&&m| m >= idx).count();
        if replicas >= majority {
            best = idx;
        }
    }
    best
}
