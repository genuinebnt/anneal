from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []


def entries(terms, base=100):
    return "vec![" + ", ".join(f"Entry {{ term: {t}, cmd: {base + i} }}" for i, t in enumerate(terms)) + "]"


def terms_of(n):
    return f"{n}.log.iter().map(|e| e.term).collect::<Vec<u64>>()"


# ---------------------------------------------------------------------------------------------------------------
# 1. Voting
VOTE_TYPES = """
/// One log entry: the term of the leader that created it, and a command for the state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub term: u64,
    pub cmd: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteRequest {
    pub term: u64,
    pub candidate: u32,
    /// Index (1-based) and term of the candidate's last log entry; `0, 0` for an empty log.
    pub last_log_index: u64,
    pub last_log_term: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteReply {
    pub term: u64,
    pub granted: bool,
}

/// The persistent state a Raft server needs to vote. `log[0]` is entry number 1.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub term: u64,
    pub voted_for: Option<u32>,
    pub log: Vec<Entry>,
}
"""

VOTE_STARTER = VOTE_TYPES + """
impl Node {
    /// Handles `RequestVote` from a candidate and returns the reply. Mutates the term and the vote as Raft requires.
    pub fn handle_vote_request(&mut self, req: &VoteRequest) -> VoteReply {
        todo!()
    }
}
"""

VOTE = VOTE_TYPES + """
impl Node {
    pub fn handle_vote_request(&mut self, req: &VoteRequest) -> VoteReply {
        if req.term < self.term {
            return VoteReply { term: self.term, granted: false };
        }
        if req.term > self.term {
            self.term = req.term;
            self.voted_for = None; // a new term: nobody has been voted for in it yet
        }
        let my_last_term = self.log.last().map_or(0, |e| e.term);
        let my_last_index = self.log.len() as u64;
        // The candidate's log must be at least as up to date as ours: later last term wins, then the longer log.
        let up_to_date = req.last_log_term > my_last_term || (req.last_log_term == my_last_term && req.last_log_index >= my_last_index);
        let free = self.voted_for.map_or(true, |v| v == req.candidate);
        let granted = up_to_date && free;
        if granted {
            self.voted_for = Some(req.candidate);
        }
        VoteReply { term: self.term, granted }
    }
}
"""


def VN(term, voted, log_terms):
    return f"Node {{ term: {term}, voted_for: {voted}, log: {entries(log_terms)} }}"


def VR(term, cand, li, lt):
    return f"VoteRequest {{ term: {term}, candidate: {cand}, last_log_index: {li}, last_log_term: {lt} }}"


def RP(term, ok):
    return f"VoteReply {{ term: {term}, granted: {ok} }}"


P.append(dict(
    slug="raft-vote-rules", title="Raft's RequestVote: who may become leader", level="medium", stage="rules",
    tags=["Raft", "consensus", "leader election", "terms", "RequestVote", "election restriction", "distributed systems"],
    teaches=["A Raft **term** is a logical clock: any message with a newer term makes the receiver adopt it (and forget its vote); an older term is ignored.",
             "Each server votes at most once per term, and the vote must survive a retry: the same candidate asking again gets the same answer.",
             "The **election restriction** (the candidate's log must be at least as up to date: later last term first, then longer) is what guarantees a new leader already holds every committed entry."],
    statement="""
        In Raft, a server that has not heard from a leader becomes a candidate and sends `RequestVote` to the others. Implement the
        receiving side as a pure state transition: `Node::handle_vote_request` updates `term` / `voted_for` and returns the reply.

        The rules from the Raft paper (§5.1 and §5.4.1):

        1. A request from an older term is denied, and the reply carries our (newer) term.
        2. A request from a **newer** term makes us adopt that term and clear `voted_for`, *whether or not* we then grant the vote.
        3. We grant the vote if we have not voted for someone else in this term (voting for the same candidate again is fine)
           **and** the candidate's log is at least as up to date as ours: compare the **terms** of the last entries first (the
           later term wins); if they are equal, the longer log (higher last index) wins. An empty log has last term 0, index 0.
        4. Granting records `voted_for`. The reply always carries our term as it stands after the request.
    """,
    starter=VOTE_STARTER,
    solution=VOTE,
    visible=[
        T("fresh_node_grants", "empty logs, newer term", "{ let mut n = Node::default(); let r = n.handle_vote_request(&" + VR(1, 7, 0, 0) + "); (r, n.term, n.voted_for) }", "(" + RP(1, "true") + ", 1, Some(7))"),
        T("older_term_is_denied", "stale candidate", "{ let mut n = " + VN(5, "None", []) + "; let r = n.handle_vote_request(&" + VR(3, 7, 0, 0) + "); (r, n.term, n.voted_for) }", "(" + RP(5, "false") + ", 5, None)"),
        T("one_vote_per_term", "a second candidate in the same term", "{ let mut n = Node::default(); let a = n.handle_vote_request(&" + VR(1, 1, 0, 0) + "); let b = n.handle_vote_request(&" + VR(1, 2, 0, 0) + "); (a.granted, b.granted, n.voted_for) }", "(true, false, Some(1))"),
        T("same_candidate_may_retry", "the reply was lost, the request is resent", "{ let mut n = Node::default(); let a = n.handle_vote_request(&" + VR(1, 1, 0, 0) + "); let b = n.handle_vote_request(&" + VR(1, 1, 0, 0) + "); (a.granted, b.granted) }", "(true, true)"),
        T("newer_term_forgets_the_vote", "voted in term 1, asked in term 2", "{ let mut n = " + VN(1, "Some(1)", []) + "; let r = n.handle_vote_request(&" + VR(2, 2, 0, 0) + "); (r, n.voted_for) }", "(" + RP(2, "true") + ", Some(2))"),
        T("shorter_log_in_the_same_term_is_denied", "candidate is behind", "{ let mut n = " + VN(2, "None", [1, 2]) + "; n.handle_vote_request(&" + VR(3, 9, 1, 1) + ").granted }", "false"),
    ],
    hidden=[
        T("a_denied_vote_still_adopts_the_term", "term 3 request with a stale log", "{ let mut n = " + VN(1, "Some(5)", [1, 1]) + "; let r = n.handle_vote_request(&" + VR(3, 6, 0, 0) + "); (r, n.term, n.voted_for) }", "(" + RP(3, "false") + ", 3, None)"),
        T("later_last_term_beats_a_longer_log", "the candidate's last entry is from a newer term", "{ let mut n = " + VN(2, "None", [1, 1, 1, 1]) + "; n.handle_vote_request(&" + VR(3, 9, 1, 2) + ").granted }", "true"),
        T("older_last_term_loses_even_if_longer", "length is only a tiebreak", "{ let mut n = " + VN(2, "None", [2]) + "; n.handle_vote_request(&" + VR(3, 9, 5, 1) + ").granted }", "false"),
        T("equal_last_term_compares_length", "index 3 vs 3 grants, 2 vs 3 denies", "(" + "{ let mut n = " + VN(2, "None", [1, 1, 2]) + "; n.handle_vote_request(&" + VR(3, 9, 3, 2) + ").granted }, " + "{ let mut n = " + VN(2, "None", [1, 1, 2]) + "; n.handle_vote_request(&" + VR(3, 9, 2, 2) + ").granted }, " + "{ let mut n = " + VN(2, "None", [1, 1, 2]) + "; n.handle_vote_request(&" + VR(3, 9, 4, 2) + ").granted })", "(true, false, true)"),
        T("empty_logs_are_equal", "0, 0 vs 0, 0", "{ let mut n = Node::default(); n.handle_vote_request(&" + VR(1, 3, 0, 0) + ").granted }", "true"),
        T("a_candidate_with_an_empty_log_loses_to_a_voter_with_entries", "0, 0 vs 1, 1", "{ let mut n = " + VN(1, "None", [1]) + "; n.handle_vote_request(&" + VR(2, 3, 0, 0) + ").granted }", "false"),
        T("a_denied_candidate_does_not_take_the_vote", "voted_for stays empty", "{ let mut n = " + VN(1, "None", [1, 1]) + "; n.handle_vote_request(&" + VR(1, 3, 1, 1) + "); n.voted_for }", "None"),
        T("equal_term_keeps_the_vote_after_a_denial", "voted for 4, candidate 5 asks", "{ let mut n = " + VN(2, "Some(4)", []) + "; let r = n.handle_vote_request(&" + VR(2, 5, 0, 0) + "); (r, n.voted_for) }", "(" + RP(2, "false") + ", Some(4))"),
        T("a_stale_request_changes_nothing", "term 1 against term 4", "{ let mut n = " + VN(4, "Some(2)", [4]) + "; let r = n.handle_vote_request(&" + VR(1, 9, 5, 4) + "); (r, n.term, n.voted_for, n.log.len()) }", "(" + RP(4, "false") + ", 4, Some(2), 1)"),
        T("terms_only_move_forward", "term 2 request to a term 9 node", "{ let mut n = " + VN(9, "None", []) + "; n.handle_vote_request(&" + VR(2, 1, 0, 0) + "); n.term }", "9"),
    ],
    wrong=dict(
        ignores_the_log=sub(VOTE, "let granted = up_to_date && free;", "let granted = free;"),
        length_before_term=sub(VOTE, "let up_to_date = req.last_log_term > my_last_term || (req.last_log_term == my_last_term && req.last_log_index >= my_last_index);", "let up_to_date = req.last_log_index > my_last_index || (req.last_log_index == my_last_index && req.last_log_term >= my_last_term);"),
        never_adopts_the_term=sub(VOTE, "            self.term = req.term;\n            self.voted_for = None; // a new term: nobody has been voted for in it yet\n", ""),
        vote_is_forgotten=sub(VOTE, "        if granted {\n            self.voted_for = Some(req.candidate);\n        }\n", ""),
        votes_twice=sub(VOTE, "let free = self.voted_for.map_or(true, |v| v == req.candidate);", "let free = true;"),
        grants_a_stale_term=sub(VOTE, "        if req.term < self.term {\n            return VoteReply { term: self.term, granted: false };\n        }\n", ""),
        newer_term_keeps_the_old_vote=sub(VOTE, "            self.voted_for = None; // a new term: nobody has been voted for in it yet\n", ""),
        strictly_longer_required=sub(VOTE, "req.last_log_index >= my_last_index", "req.last_log_index > my_last_index"),
    ),
    hints=[("approach", "Do the term bookkeeping first (reject older, adopt newer and clear the vote), then decide on the vote with two independent conditions: 'have I not voted for someone else' and 'is the candidate's log at least as up to date'. Only a grant records `voted_for`."),
           ("rust", "`self.log.last().map_or(0, |e| e.term)` is the last term (0 for an empty log) and `self.log.len() as u64` is the last index. `self.voted_for.map_or(true, |v| v == req.candidate)` reads as 'free, or already ours'. The up-to-date test is a tuple comparison in disguise: `(req.last_log_term, req.last_log_index) >= (my_last_term, my_last_index)`."),
           ("edge", "A request with a newer term clears the vote even when you then deny it (the candidate's log may be stale). Compare terms *before* lengths: a long log full of old-term entries must lose to a short log whose last entry is newer.")],
    notes=("This is `RequestVote` from the Raft paper, word for word; etcd/raft and TiKV's raft-rs implement it as exactly this kind of pure `Step` function so that it can be tested deterministically. The two persisted fields (`term`, `voted_for`) must hit disk before the reply is sent, or a restart could vote twice in one term and elect two leaders.", "O(1)", "O(1)"),
    follow_up="1) Add the *pre-vote* phase (a candidate first asks 'would you vote for me?' without bumping terms) and show how it stops a partitioned node from disrupting a healthy cluster when it rejoins. 2) Write the candidate side: count votes, step down on a newer term, restart on timeout with a randomised election timeout. 3) Explain with a concrete five-node scenario why 'longest log wins' alone (ignoring terms) can elect a leader that lacks a committed entry. 4) Add `voted_for` persistence behind a trait and test a crash between the vote and the reply. 5) Move on to log replication in the next problem.",
    related=["Y7", "C2", "B4"],
))

# ---------------------------------------------------------------------------------------------------------------
# 2. AppendEntries
APP_TYPES = """
/// One log entry: the term of the leader that created it, and a command for the state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub term: u64,
    pub cmd: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendRequest {
    pub term: u64,
    pub leader: u32,
    /// The entry (1-based index and its term) the new entries follow; `0, 0` means "from the very start".
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub entries: Vec<Entry>,
    pub leader_commit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendReply {
    pub term: u64,
    pub success: bool,
    /// On success: the index of the last entry known to match the leader's log (`prev_log_index + entries.len()`). 0 on failure.
    pub match_index: u64,
}

/// A follower's persistent state plus its commit index. `log[0]` is entry number 1.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub term: u64,
    pub voted_for: Option<u32>,
    pub log: Vec<Entry>,
    pub commit_index: u64,
}
"""

APP_STARTER = APP_TYPES + """
impl Node {
    /// Handles `AppendEntries` from a leader (heartbeats are requests with no entries).
    pub fn handle_append(&mut self, req: &AppendRequest) -> AppendReply {
        todo!()
    }
}
"""

APP = APP_TYPES + """
impl Node {
    pub fn handle_append(&mut self, req: &AppendRequest) -> AppendReply {
        if req.term < self.term {
            return AppendReply { term: self.term, success: false, match_index: 0 };
        }
        if req.term > self.term {
            self.term = req.term;
            self.voted_for = None;
        }
        let p = req.prev_log_index;
        if p > 0 {
            match self.log.get(p as usize - 1) {
                Some(e) if e.term == req.prev_log_term => {}
                _ => return AppendReply { term: self.term, success: false, match_index: 0 },
            }
        }
        for (k, e) in req.entries.iter().enumerate() {
            let pos = p as usize + k;
            if pos < self.log.len() {
                if self.log[pos].term != e.term {
                    self.log.truncate(pos); // a conflicting suffix: drop it and everything after
                    self.log.push(e.clone());
                }
                // same term at the same index: it is the same entry, leave it (a delayed duplicate must not shorten the log)
            } else {
                self.log.push(e.clone());
            }
        }
        let last_new = p + req.entries.len() as u64;
        if req.leader_commit > self.commit_index {
            self.commit_index = self.commit_index.max(req.leader_commit.min(last_new));
        }
        AppendReply { term: self.term, success: true, match_index: last_new }
    }
}
"""


def AN(term, voted, log_terms, commit):
    return f"Node {{ term: {term}, voted_for: {voted}, log: {entries(log_terms, 1)}, commit_index: {commit} }}"


def AR(term, prev_i, prev_t, ent_terms, commit, base=100):
    return f"AppendRequest {{ term: {term}, leader: 1, prev_log_index: {prev_i}, prev_log_term: {prev_t}, entries: {entries(ent_terms, base)}, leader_commit: {commit} }}"


def AP(term, ok, m):
    return f"AppendReply {{ term: {term}, success: {ok}, match_index: {m} }}"


P.append(dict(
    slug="raft-append-entries", title="Raft's AppendEntries: the follower's side of log replication", level="hard", stage="rules",
    tags=["Raft", "log replication", "AppendEntries", "consistency check", "log matching", "commit index", "idempotence"],
    teaches=["The **log matching property**: if a follower has an entry with the same index *and term* as the leader's `prev` entry, the two logs are identical up to there. The consistency check is how a leader finds that point.",
             "Only a *conflicting* entry (same index, different term) is deleted, along with everything after it. A request carrying entries the follower already has must change nothing: it may be a delayed duplicate.",
             "The commit index only moves forward, and never past the last entry this request vouched for: the follower's own tail beyond it may be stale."],
    statement="""
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
    """,
    starter=APP_STARTER,
    solution=APP,
    visible=[
        T("heartbeat_on_an_empty_node", "no entries, newer term", "{ let mut n = Node::default(); let r = n.handle_append(&" + AR(1, 0, 0, [], 0) + "); (r, n.term, n.log.len()) }", "(" + AP(1, "true", 0) + ", 1, 0)"),
        T("append_to_an_empty_log", "two entries from the start", "{ let mut n = Node::default(); let r = n.handle_append(&" + AR(1, 0, 0, [1, 1], 0) + "); (r, " + terms_of("n") + ") }", "(" + AP(1, "true", 2) + ", vec![1, 1])"),
        T("prev_entry_missing_is_rejected", "the log is too short", "{ let mut n = " + AN(1, "None", [1], 0) + "; let r = n.handle_append(&" + AR(1, 2, 1, [1], 0) + "); (r, n.log.len()) }", "(" + AP(1, "false", 0) + ", 1)"),
        T("prev_term_mismatch_is_rejected", "same index, different term", "{ let mut n = " + AN(2, "None", [1, 1], 0) + "; let r = n.handle_append(&" + AR(2, 2, 2, [2], 0) + "); (r, n.log.len()) }", "(" + AP(2, "false", 0) + ", 2)"),
        T("a_conflicting_entry_truncates_the_tail", "term 2 replaces a term 1 suffix", "{ let mut n = " + AN(1, "None", [1, 1, 1], 0) + "; let r = n.handle_append(&" + AR(2, 1, 1, [2], 0) + "); (r.success, " + terms_of("n") + ") }", "(true, vec![1, 2])"),
        T("stale_term_is_rejected", "term 3 against a term 5 follower", "{ let mut n = " + AN(5, "None", [1], 0) + "; let r = n.handle_append(&" + AR(3, 0, 0, [3], 0) + "); (r, n.term, n.log.len()) }", "(" + AP(5, "false", 0) + ", 5, 1)"),
    ],
    hidden=[
        T("a_delayed_duplicate_does_not_shorten_the_log", "the first two entries again, after four arrived", "{ let mut n = " + AN(1, "None", [1, 1, 1, 1], 0) + "; let r = n.handle_append(&" + AR(1, 0, 0, [1, 1], 0) + "); (r, n.log.len()) }", "(" + AP(1, "true", 2) + ", 4)"),
        T("commit_stops_at_the_last_covered_entry", "our stale tail is not committed", "{ let mut n = " + AN(2, "None", [1, 1, 2], 0) + "; n.handle_append(&" + AR(3, 1, 1, [], 3) + "); n.commit_index }", "1"),
        T("commit_follows_the_leader_when_covered", "three entries, leader committed two", "{ let mut n = Node::default(); n.handle_append(&" + AR(1, 0, 0, [1, 1, 1], 2) + "); n.commit_index }", "2"),
        T("commit_is_capped_by_the_new_entries", "leader committed 9, request covers 3", "{ let mut n = Node::default(); n.handle_append(&" + AR(1, 0, 0, [1, 1, 1], 9) + "); n.commit_index }", "3"),
        T("commit_never_decreases", "a stale heartbeat with a big leader_commit", "{ let mut n = " + AN(1, "None", [1, 1, 1], 3) + "; n.handle_append(&" + AR(1, 0, 0, [], 5) + "); n.commit_index }", "3"),
        T("a_higher_term_clears_the_vote", "term 2 leader, we voted in term 1", "{ let mut n = " + AN(1, "Some(7)", [], 0) + "; n.handle_append(&" + AR(2, 0, 0, [], 0) + "); (n.term, n.voted_for) }", "(2, None)"),
        T("the_same_term_keeps_the_vote", "equal term leader", "{ let mut n = " + AN(2, "Some(7)", [], 0) + "; n.handle_append(&" + AR(2, 0, 0, [], 0) + "); (n.term, n.voted_for) }", "(2, Some(7))"),
        T("prev_index_zero_replaces_a_conflicting_first_entry", "no prev check at the start", "{ let mut n = " + AN(1, "None", [1], 0) + "; let r = n.handle_append(&" + AR(2, 0, 0, [2, 2], 0) + "); (r.success, " + terms_of("n") + ") }", "(true, vec![2, 2])"),
        T("a_failed_check_changes_nothing", "no partial writes, no commit", "{ let mut n = " + AN(1, "None", [1, 1], 1) + "; let r = n.handle_append(&" + AR(1, 5, 1, [1, 1], 2) + "); (r.success, " + terms_of("n") + ", n.commit_index) }", "(false, vec![1, 1], 1)"),
        T("append_after_a_truncation_continues", "term 2 suffix of three", "{ let mut n = " + AN(1, "None", [1, 1, 1, 1], 0) + "; let r = n.handle_append(&" + AR(2, 1, 1, [2, 2, 2], 0) + "); (r.match_index, " + terms_of("n") + ") }", "(4, vec![1, 2, 2, 2])"),
        T("match_index_is_what_the_request_covered", "the follower has more", "{ let mut n = " + AN(1, "None", [1, 1, 1, 1], 0) + "; n.handle_append(&" + AR(1, 1, 1, [1], 0) + ").match_index }", "2"),
        T("resending_the_same_request_is_idempotent", "twice, same result", "{ let mut n = Node::default(); let q = " + AR(1, 0, 0, [1, 2], 2) + "; let a = n.handle_append(&q); let b = n.handle_append(&q); (a == b, " + terms_of("n") + ", n.commit_index) }", "(true, vec![1, 2], 2)"),
    ],
    wrong=dict(
        stale_term_accepted=sub(APP, "        if req.term < self.term {\n            return AppendReply { term: self.term, success: false, match_index: 0 };\n        }\n", ""),
        skips_the_prev_term_check=sub(APP, "Some(e) if e.term == req.prev_log_term => {}", "Some(_) => {}"),
        truncates_on_every_append=sub(APP, """        for (k, e) in req.entries.iter().enumerate() {
            let pos = p as usize + k;
            if pos < self.log.len() {
                if self.log[pos].term != e.term {
                    self.log.truncate(pos); // a conflicting suffix: drop it and everything after it
                    self.log.push(e.clone());
                }
                // same term at the same index: it is the same entry, leave it (a delayed duplicate must not shorten the log)
            } else {
                self.log.push(e.clone());
            }
        }
""".replace("everything after it", "everything after"), "        self.log.truncate(p as usize);\n        self.log.extend(req.entries.iter().cloned());\n"),
        never_replaces_conflicts=sub(APP, "                if self.log[pos].term != e.term {\n                    self.log.truncate(pos); // a conflicting suffix: drop it and everything after\n                    self.log.push(e.clone());\n                }\n", ""),
        commit_past_the_covered_entries=sub(APP, "req.leader_commit.min(last_new)", "req.leader_commit.min(self.log.len() as u64)"),
        commit_can_decrease=sub(APP, "self.commit_index = self.commit_index.max(req.leader_commit.min(last_new));", "self.commit_index = req.leader_commit.min(last_new);"),
        never_adopts_the_term=sub(APP, "            self.term = req.term;\n            self.voted_for = None;\n", ""),
        newer_term_keeps_the_vote=sub(APP, "            self.term = req.term;\n            self.voted_for = None;\n", "            self.term = req.term;\n"),
        wrong_match_index=sub(APP, "AppendReply { term: self.term, success: true, match_index: last_new }", "AppendReply { term: self.term, success: true, match_index: self.log.len() as u64 }"),
    ),
    hints=[("approach", "Four steps in order: term check, consistency check on `prev`, merge the entries one by one (append past the end, truncate only on a term *conflict*, keep matches), then advance the commit index. A failed check must return before touching the log."),
           ("rust", "`self.log.get(p as usize - 1)` is the entry at 1-based index `p` (guard `p > 0` first). Use the 0-based position `p + k` for the `k`-th new entry; `Vec::truncate(pos)` drops `pos..`. For the commit index, `commit_index = commit_index.max(leader_commit.min(last_new))` is monotone by construction."),
           ("edge", "Why not just `truncate(prev)` then extend? Because a reordered older request would then delete entries a newer request already delivered (and maybe acknowledged). And why `min(leader_commit, last_new)` rather than the log length? The follower may hold a stale tail from an old leader that this request has not yet overwritten: committing it would be wrong.")],
    notes=("This is Figure 2 of the Raft paper and the core of every Raft library (etcd, TiKV, Consul, rqlite). The mistakes tested here (truncating on duplicates, committing a stale tail, letting the commit index go backwards) are the classic ones that pass happy-path tests and corrupt data after a partition or a reordered packet. Real implementations add a conflict hint to the reply (term and first index of the conflicting term) so the leader can back up by a term at a time instead of one entry.", "O(entries) per request", "O(1) extra"),
    follow_up="1) Add the conflict hint (`conflict_term`, `conflict_index`) to `AppendReply` and write the leader's `next_index` back-off that uses it: how many round trips does it save after a long partition? 2) Persist the log and term to a file before replying (write-ahead) and test a crash between append and reply. 3) Write a randomised test: a leader's log, a follower's divergent log, deliver the leader's messages duplicated and reordered, and check the follower's log always converges to a prefix of the leader's. 4) Add log compaction (a snapshot replaces entries up to an index) and see which of these rules change. 5) The leader side comes next: how it decides what is committed.",
    related=["Y7", "B4", "C2"],
))

# ---------------------------------------------------------------------------------------------------------------
# 3. Leader commit
COMMIT_STARTER = """
/// The leader's commit rule. `log_terms[i]` is the term of entry `i + 1` in the leader's log; `follower_match[j]` is the highest
/// index known to be replicated on follower `j` (the leader itself holds its whole log). Returns the new commit index.
pub fn advance_commit(log_terms: &[u64], current_term: u64, follower_match: &[u64], commit_index: u64) -> u64 {
    todo!()
}
"""

COMMIT = """
pub fn advance_commit(log_terms: &[u64], current_term: u64, follower_match: &[u64], commit_index: u64) -> u64 {
    let cluster = follower_match.len() + 1; // the followers and the leader
    let majority = cluster / 2 + 1;
    let mut best = commit_index;
    for idx in (commit_index + 1)..=(log_terms.len() as u64) {
        // only an entry of the leader's own term is committed by counting replicas (the Figure 8 rule)
        if log_terms[idx as usize - 1] != current_term {
            continue;
        }
        let replicas = 1 + follower_match.iter().filter(|&&m| m >= idx).count();
        if replicas >= majority {
            best = idx;
        }
    }
    best
}
"""

P.append(dict(
    slug="raft-leader-commit", title="When a Raft leader may commit: majorities and the Figure 8 rule", level="hard", stage="rules",
    tags=["Raft", "commit index", "quorum", "majority", "Figure 8", "match index", "safety"],
    teaches=["An entry is committed once a **majority** of servers store it: with `n` servers that is `n / 2 + 1` (3 of 4, 3 of 5), counting the leader itself.",
             "The subtle rule: a leader may only commit by counting replicas an entry **from its own term**. Entries from earlier terms become committed indirectly, when a later entry on top of them is. Counting old-term entries can lose committed data (Raft paper, Figure 8).",
             "Committing index `N` commits every entry before it, so the answer is the *largest* qualifying index."],
    statement="""
        Write the leader's commit rule as a pure function. `log_terms[i]` is the term of entry `i + 1` of the leader's log,
        `follower_match[j]` is the highest log index known to be replicated on follower `j`, and the leader itself holds its
        entire log. The cluster is the followers plus the leader.

        Return the highest `N > commit_index`, `N <= log_terms.len()`, such that **a majority** of the cluster stores entry `N`
        **and `log_terms[N - 1] == current_term`**; if there is none, return `commit_index` unchanged. A majority of `n` servers is
        `n / 2 + 1`, and the leader counts as one of them.

        The term condition is the heart of it: an entry left over from an earlier term may be on a majority and *still* be
        overwritten by a future leader, so it must not be committed by counting. It is committed implicitly when a current-term entry
        after it is.
    """,
    starter=COMMIT_STARTER,
    solution=COMMIT,
    visible=[
        T("majority_of_three", "leader + one follower is enough", "advance_commit(&[1, 1, 1], 1, &[3, 2], 0)", "3"),
        T("partial_replication", "index 1 has a majority, 2 and 3 do not", "advance_commit(&[1, 1, 1], 1, &[1, 0], 0)", "1"),
        T("no_majority_keeps_the_commit_index", "only the leader has it", "advance_commit(&[1, 1], 1, &[0, 0], 0)", "0"),
        T("five_servers_need_three", "two copies is not enough, three is", "(advance_commit(&[1, 1, 1], 1, &[3, 0, 0, 0], 0), advance_commit(&[1, 1, 1], 1, &[3, 3, 0, 0], 0))", "(0, 3)"),
        T("old_term_entries_are_not_counted", "Figure 8: entry 3 is from term 2", "advance_commit(&[1, 1, 2], 3, &[3, 3], 0)", "0"),
        T("a_current_term_entry_commits_what_is_below_it", "index 4 is term 3", "advance_commit(&[1, 1, 2, 3], 3, &[4, 4], 0)", "4"),
    ],
    hidden=[
        T("four_servers_need_three", "2 of 4 is not a majority", "(advance_commit(&[1, 1], 1, &[2, 0, 0], 0), advance_commit(&[1, 1], 1, &[2, 2, 0], 0))", "(0, 2)"),
        T("a_single_node_cluster_commits_alone", "no followers", "advance_commit(&[1, 1], 1, &[], 0)", "2"),
        T("an_empty_log_commits_nothing", "no entries", "advance_commit(&[], 1, &[0, 0], 0)", "0"),
        T("the_largest_qualifying_index_wins", "not the first one", "advance_commit(&[1, 1, 1, 1], 1, &[4, 2], 0)", "4"),
        T("never_goes_backwards", "already committed past what the followers report", "(advance_commit(&[1, 1, 1], 1, &[1, 1], 3), advance_commit(&[1, 1, 1], 1, &[3, 3], 2))", "(3, 3)"),
        T("stops_where_replication_stops", "indexes 2 and 3 are on a majority, 4 is not", "advance_commit(&[1, 3, 3, 3], 3, &[3, 1], 0)", "3"),
        T("old_entries_in_the_middle_are_skipped_but_not_blocking", "term 3 entries after term 1", "advance_commit(&[1, 3, 3], 3, &[3, 3], 0)", "3"),
        T("only_old_entries_replicated", "the new term has none yet", "advance_commit(&[1, 2, 2], 3, &[3, 3, 3, 3], 1)", "1"),
        T("the_leader_counts_as_a_replica", "one follower of three is a majority with the leader", "advance_commit(&[2], 2, &[1, 0], 0)", "1"),
        T("future_terms_are_never_current", "terms above current_term do not count either", "advance_commit(&[2, 5], 2, &[2, 2], 0)", "1"),
    ],
    wrong=dict(
        counts_old_terms=sub(COMMIT, "        if log_terms[idx as usize - 1] != current_term {\n            continue;\n        }\n", ""),
        forgets_the_leader=sub(COMMIT, "let replicas = 1 + follower_match", "let replicas = follower_match"),
        half_is_enough=sub(COMMIT, "if replicas >= majority {", "if replicas * 2 >= cluster {"),
        takes_the_first_index=sub(COMMIT, "        if replicas >= majority {\n            best = idx;\n        }", "        if replicas >= majority {\n            return idx;\n        }"),
        commit_index_can_shrink=sub(COMMIT, "    let mut best = commit_index;", "    let mut best = 0;"),
    ),
    hints=[("approach", "For each candidate index `N` above the current commit index: skip it unless its entry has the current term; count the followers whose match index is at least `N`, add one for the leader, compare with the majority. Keep the largest passing `N`."),
           ("rust", "`follower_match.iter().filter(|&&m| m >= idx).count()` counts replicas; the cluster size is `follower_match.len() + 1` and the majority is `cluster / 2 + 1` (integer division: 5 servers need 3, 4 need 3, 1 needs 1). Iterate `(commit_index + 1)..=len` and remember that `log_terms[idx - 1]` is the term of entry `idx`."),
           ("edge", "The term check is only on the entry at `N` itself, not on the ones below it: committing `N` commits all earlier entries regardless of their terms. And the result never drops below `commit_index`: if nothing new qualifies, return what you were given.")],
    notes=("This is the rule that Figure 8 of the Raft paper exists to justify, and the one that most home-grown implementations get wrong: etcd's `maybeCommit` checks `term == current term` for exactly this reason. Leaders commonly append a no-op entry at the start of their term so that earlier entries become committed quickly once it replicates.", "O(log length * followers)", "O(1)"),
    follow_up="1) Make it O(followers log followers): sort the match indexes (with the leader's own last index) and take the median-like element, then check its term. Prove it equals the loop version. 2) Build the Figure 8 scenario as a test with five servers S1..S5 and show a committed-by-counting entry that a later leader overwrites; then show the same schedule is safe with this rule. 3) Add the leader's `next_index` / `match_index` bookkeeping on `AppendReply` (success: raise match; failure: back next_index off) and drive it from the previous problem's follower. 4) Add joint-consensus style *two majorities* (old and new configuration must each have one) and extend the function. 5) Why does a leader append a no-op on election?",
    related=["Y7", "C2", "B4"],
))

STAGES = [("rules", "The rules", "hard")]

if __name__ == "__main__":
    n = write_track("y7-raft-consensus", "Y7", "Raft consensus rules", "Y", "sde3", 7,
                    "The decisions at the core of Raft as pure functions: who may win an election, how a follower merges a log, and when a leader may commit.",
                    STAGES, P)
    print("Y7", n)
