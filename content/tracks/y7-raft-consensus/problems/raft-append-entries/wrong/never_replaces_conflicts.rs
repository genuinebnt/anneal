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
