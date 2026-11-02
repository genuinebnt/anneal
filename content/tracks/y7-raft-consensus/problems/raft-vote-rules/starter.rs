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

impl Node {
    /// Handles `RequestVote` from a candidate and returns the reply. Mutates the term and the vote as Raft requires.
    pub fn handle_vote_request(&mut self, req: &VoteRequest) -> VoteReply {
        todo!()
    }
}
