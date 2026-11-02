use solution::*;

#[test]
fn a_denied_vote_still_adopts_the_term() {
    check!(r#"term 3 request with a stale log"#, { let mut n = Node { term: 1, voted_for: Some(5), log: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }] }; let r = n.handle_vote_request(&VoteRequest { term: 3, candidate: 6, last_log_index: 0, last_log_term: 0 }); (r, n.term, n.voted_for) }, (VoteReply { term: 3, granted: false }, 3, None));
}

#[test]
fn later_last_term_beats_a_longer_log() {
    check!(r#"the candidate's last entry is from a newer term"#, { let mut n = Node { term: 2, voted_for: None, log: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }, Entry { term: 1, cmd: 102 }, Entry { term: 1, cmd: 103 }] }; n.handle_vote_request(&VoteRequest { term: 3, candidate: 9, last_log_index: 1, last_log_term: 2 }).granted }, true);
}

#[test]
fn older_last_term_loses_even_if_longer() {
    check!(r#"length is only a tiebreak"#, { let mut n = Node { term: 2, voted_for: None, log: vec![Entry { term: 2, cmd: 100 }] }; n.handle_vote_request(&VoteRequest { term: 3, candidate: 9, last_log_index: 5, last_log_term: 1 }).granted }, false);
}

#[test]
fn equal_last_term_compares_length() {
    check!(r#"index 3 vs 3 grants, 2 vs 3 denies"#, ({ let mut n = Node { term: 2, voted_for: None, log: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }, Entry { term: 2, cmd: 102 }] }; n.handle_vote_request(&VoteRequest { term: 3, candidate: 9, last_log_index: 3, last_log_term: 2 }).granted }, { let mut n = Node { term: 2, voted_for: None, log: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }, Entry { term: 2, cmd: 102 }] }; n.handle_vote_request(&VoteRequest { term: 3, candidate: 9, last_log_index: 2, last_log_term: 2 }).granted }, { let mut n = Node { term: 2, voted_for: None, log: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }, Entry { term: 2, cmd: 102 }] }; n.handle_vote_request(&VoteRequest { term: 3, candidate: 9, last_log_index: 4, last_log_term: 2 }).granted }), (true, false, true));
}

#[test]
fn empty_logs_are_equal() {
    check!(r#"0, 0 vs 0, 0"#, { let mut n = Node::default(); n.handle_vote_request(&VoteRequest { term: 1, candidate: 3, last_log_index: 0, last_log_term: 0 }).granted }, true);
}

#[test]
fn a_candidate_with_an_empty_log_loses_to_a_voter_with_entries() {
    check!(r#"0, 0 vs 1, 1"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 100 }] }; n.handle_vote_request(&VoteRequest { term: 2, candidate: 3, last_log_index: 0, last_log_term: 0 }).granted }, false);
}

#[test]
fn a_denied_candidate_does_not_take_the_vote() {
    check!(r#"voted_for stays empty"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }] }; n.handle_vote_request(&VoteRequest { term: 1, candidate: 3, last_log_index: 1, last_log_term: 1 }); n.voted_for }, None);
}

#[test]
fn equal_term_keeps_the_vote_after_a_denial() {
    check!(r#"voted for 4, candidate 5 asks"#, { let mut n = Node { term: 2, voted_for: Some(4), log: vec![] }; let r = n.handle_vote_request(&VoteRequest { term: 2, candidate: 5, last_log_index: 0, last_log_term: 0 }); (r, n.voted_for) }, (VoteReply { term: 2, granted: false }, Some(4)));
}

#[test]
fn a_stale_request_changes_nothing() {
    check!(r#"term 1 against term 4"#, { let mut n = Node { term: 4, voted_for: Some(2), log: vec![Entry { term: 4, cmd: 100 }] }; let r = n.handle_vote_request(&VoteRequest { term: 1, candidate: 9, last_log_index: 5, last_log_term: 4 }); (r, n.term, n.voted_for, n.log.len()) }, (VoteReply { term: 4, granted: false }, 4, Some(2), 1));
}

#[test]
fn terms_only_move_forward() {
    check!(r#"term 2 request to a term 9 node"#, { let mut n = Node { term: 9, voted_for: None, log: vec![] }; n.handle_vote_request(&VoteRequest { term: 2, candidate: 1, last_log_index: 0, last_log_term: 0 }); n.term }, 9);
}
