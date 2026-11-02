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
    pub fn handle_vote_request(&mut self, req: &VoteRequest) -> VoteReply {
        if req.term < self.term {
            return VoteReply { term: self.term, granted: false };
        }
        if req.term > self.term {
            self.term = req.term;
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
