use solution::*;

#[test]
fn fresh_node_grants() {
    check!(r#"empty logs, newer term"#, { let mut n = Node::default(); let r = n.handle_vote_request(&VoteRequest { term: 1, candidate: 7, last_log_index: 0, last_log_term: 0 }); (r, n.term, n.voted_for) }, (VoteReply { term: 1, granted: true }, 1, Some(7)));
}

#[test]
fn older_term_is_denied() {
    check!(r#"stale candidate"#, { let mut n = Node { term: 5, voted_for: None, log: vec![] }; let r = n.handle_vote_request(&VoteRequest { term: 3, candidate: 7, last_log_index: 0, last_log_term: 0 }); (r, n.term, n.voted_for) }, (VoteReply { term: 5, granted: false }, 5, None));
}

#[test]
fn one_vote_per_term() {
    check!(r#"a second candidate in the same term"#, { let mut n = Node::default(); let a = n.handle_vote_request(&VoteRequest { term: 1, candidate: 1, last_log_index: 0, last_log_term: 0 }); let b = n.handle_vote_request(&VoteRequest { term: 1, candidate: 2, last_log_index: 0, last_log_term: 0 }); (a.granted, b.granted, n.voted_for) }, (true, false, Some(1)));
}

#[test]
fn same_candidate_may_retry() {
    check!(r#"the reply was lost, the request is resent"#, { let mut n = Node::default(); let a = n.handle_vote_request(&VoteRequest { term: 1, candidate: 1, last_log_index: 0, last_log_term: 0 }); let b = n.handle_vote_request(&VoteRequest { term: 1, candidate: 1, last_log_index: 0, last_log_term: 0 }); (a.granted, b.granted) }, (true, true));
}

#[test]
fn newer_term_forgets_the_vote() {
    check!(r#"voted in term 1, asked in term 2"#, { let mut n = Node { term: 1, voted_for: Some(1), log: vec![] }; let r = n.handle_vote_request(&VoteRequest { term: 2, candidate: 2, last_log_index: 0, last_log_term: 0 }); (r, n.voted_for) }, (VoteReply { term: 2, granted: true }, Some(2)));
}

#[test]
fn shorter_log_in_the_same_term_is_denied() {
    check!(r#"candidate is behind"#, { let mut n = Node { term: 2, voted_for: None, log: vec![Entry { term: 1, cmd: 100 }, Entry { term: 2, cmd: 101 }] }; n.handle_vote_request(&VoteRequest { term: 3, candidate: 9, last_log_index: 1, last_log_term: 1 }).granted }, false);
}
