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
    /// Handles `AppendEntries` from a leader (heartbeats are requests with no entries).
    pub fn handle_append(&mut self, req: &AppendRequest) -> AppendReply {
        todo!()
    }
}
