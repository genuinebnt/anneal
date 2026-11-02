use solution::*;

#[test]
fn a_delayed_duplicate_does_not_shorten_the_log() {
    check!(r#"the first two entries again, after four arrived"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }, Entry { term: 1, cmd: 2 }, Entry { term: 1, cmd: 3 }, Entry { term: 1, cmd: 4 }], commit_index: 0 }; let r = n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }], leader_commit: 0 }); (r, n.log.len()) }, (AppendReply { term: 1, success: true, match_index: 2 }, 4));
}

#[test]
fn commit_stops_at_the_last_covered_entry() {
    check!(r#"our stale tail is not committed"#, { let mut n = Node { term: 2, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }, Entry { term: 1, cmd: 2 }, Entry { term: 2, cmd: 3 }], commit_index: 0 }; n.handle_append(&AppendRequest { term: 3, leader: 1, prev_log_index: 1, prev_log_term: 1, entries: vec![], leader_commit: 3 }); n.commit_index }, 1);
}

#[test]
fn commit_follows_the_leader_when_covered() {
    check!(r#"three entries, leader committed two"#, { let mut n = Node::default(); n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }, Entry { term: 1, cmd: 102 }], leader_commit: 2 }); n.commit_index }, 2);
}

#[test]
fn commit_is_capped_by_the_new_entries() {
    check!(r#"leader committed 9, request covers 3"#, { let mut n = Node::default(); n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }, Entry { term: 1, cmd: 102 }], leader_commit: 9 }); n.commit_index }, 3);
}

#[test]
fn commit_never_decreases() {
    check!(r#"a stale heartbeat with a big leader_commit"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }, Entry { term: 1, cmd: 2 }, Entry { term: 1, cmd: 3 }], commit_index: 3 }; n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![], leader_commit: 5 }); n.commit_index }, 3);
}

#[test]
fn a_higher_term_clears_the_vote() {
    check!(r#"term 2 leader, we voted in term 1"#, { let mut n = Node { term: 1, voted_for: Some(7), log: vec![], commit_index: 0 }; n.handle_append(&AppendRequest { term: 2, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![], leader_commit: 0 }); (n.term, n.voted_for) }, (2, None));
}

#[test]
fn the_same_term_keeps_the_vote() {
    check!(r#"equal term leader"#, { let mut n = Node { term: 2, voted_for: Some(7), log: vec![], commit_index: 0 }; n.handle_append(&AppendRequest { term: 2, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![], leader_commit: 0 }); (n.term, n.voted_for) }, (2, Some(7)));
}

#[test]
fn prev_index_zero_replaces_a_conflicting_first_entry() {
    check!(r#"no prev check at the start"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }], commit_index: 0 }; let r = n.handle_append(&AppendRequest { term: 2, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![Entry { term: 2, cmd: 100 }, Entry { term: 2, cmd: 101 }], leader_commit: 0 }); (r.success, n.log.iter().map(|e| e.term).collect::<Vec<u64>>()) }, (true, vec![2, 2]));
}

#[test]
fn a_failed_check_changes_nothing() {
    check!(r#"no partial writes, no commit"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }, Entry { term: 1, cmd: 2 }], commit_index: 1 }; let r = n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 5, prev_log_term: 1, entries: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }], leader_commit: 2 }); (r.success, n.log.iter().map(|e| e.term).collect::<Vec<u64>>(), n.commit_index) }, (false, vec![1, 1], 1));
}

#[test]
fn append_after_a_truncation_continues() {
    check!(r#"term 2 suffix of three"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }, Entry { term: 1, cmd: 2 }, Entry { term: 1, cmd: 3 }, Entry { term: 1, cmd: 4 }], commit_index: 0 }; let r = n.handle_append(&AppendRequest { term: 2, leader: 1, prev_log_index: 1, prev_log_term: 1, entries: vec![Entry { term: 2, cmd: 100 }, Entry { term: 2, cmd: 101 }, Entry { term: 2, cmd: 102 }], leader_commit: 0 }); (r.match_index, n.log.iter().map(|e| e.term).collect::<Vec<u64>>()) }, (4, vec![1, 2, 2, 2]));
}

#[test]
fn match_index_is_what_the_request_covered() {
    check!(r#"the follower has more"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }, Entry { term: 1, cmd: 2 }, Entry { term: 1, cmd: 3 }, Entry { term: 1, cmd: 4 }], commit_index: 0 }; n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 1, prev_log_term: 1, entries: vec![Entry { term: 1, cmd: 100 }], leader_commit: 0 }).match_index }, 2);
}

#[test]
fn resending_the_same_request_is_idempotent() {
    check!(r#"twice, same result"#, { let mut n = Node::default(); let q = AppendRequest { term: 1, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![Entry { term: 1, cmd: 100 }, Entry { term: 2, cmd: 101 }], leader_commit: 2 }; let a = n.handle_append(&q); let b = n.handle_append(&q); (a == b, n.log.iter().map(|e| e.term).collect::<Vec<u64>>(), n.commit_index) }, (true, vec![1, 2], 2));
}
