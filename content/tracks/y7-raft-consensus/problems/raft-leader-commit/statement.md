Write the leader's commit rule as a pure function. `log_terms[i]` is the term of entry `i + 1` of the leader's log,
`follower_match[j]` is the highest log index known to be replicated on follower `j`, and the leader itself holds its
entire log. The cluster is the followers plus the leader.

Return the highest `N > commit_index`, `N <= log_terms.len()`, such that **a majority** of the cluster stores entry `N`
**and `log_terms[N - 1] == current_term`**; if there is none, return `commit_index` unchanged. A majority of `n` servers is
`n / 2 + 1`, and the leader counts as one of them.

The term condition is the heart of it: an entry left over from an earlier term may be on a majority and *still* be
overwritten by a future leader, so it must not be committed by counting. It is committed implicitly when a current-term entry
after it is.
