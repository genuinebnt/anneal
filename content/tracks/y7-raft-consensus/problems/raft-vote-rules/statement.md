In Raft, a server that has not heard from a leader becomes a candidate and sends `RequestVote` to the others. Implement the
receiving side as a pure state transition: `Node::handle_vote_request` updates `term` / `voted_for` and returns the reply.

The rules from the Raft paper (§5.1 and §5.4.1):

1. A request from an older term is denied, and the reply carries our (newer) term.
2. A request from a **newer** term makes us adopt that term and clear `voted_for`, *whether or not* we then grant the vote.
3. We grant the vote if we have not voted for someone else in this term (voting for the same candidate again is fine)
   **and** the candidate's log is at least as up to date as ours: compare the **terms** of the last entries first (the
   later term wins); if they are equal, the longer log (higher last index) wins. An empty log has last term 0, index 0.
4. Granting records `voted_for`. The reply always carries our term as it stands after the request.
