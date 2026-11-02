use solution::*;

#[test]
fn heartbeat_on_an_empty_node() {
    check!(r#"no entries, newer term"#, { let mut n = Node::default(); let r = n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![], leader_commit: 0 }); (r, n.term, n.log.len()) }, (AppendReply { term: 1, success: true, match_index: 0 }, 1, 0));
}

#[test]
fn append_to_an_empty_log() {
    check!(r#"two entries from the start"#, { let mut n = Node::default(); let r = n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![Entry { term: 1, cmd: 100 }, Entry { term: 1, cmd: 101 }], leader_commit: 0 }); (r, n.log.iter().map(|e| e.term).collect::<Vec<u64>>()) }, (AppendReply { term: 1, success: true, match_index: 2 }, vec![1, 1]));
}

#[test]
fn prev_entry_missing_is_rejected() {
    check!(r#"the log is too short"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }], commit_index: 0 }; let r = n.handle_append(&AppendRequest { term: 1, leader: 1, prev_log_index: 2, prev_log_term: 1, entries: vec![Entry { term: 1, cmd: 100 }], leader_commit: 0 }); (r, n.log.len()) }, (AppendReply { term: 1, success: false, match_index: 0 }, 1));
}

#[test]
fn prev_term_mismatch_is_rejected() {
    check!(r#"same index, different term"#, { let mut n = Node { term: 2, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }, Entry { term: 1, cmd: 2 }], commit_index: 0 }; let r = n.handle_append(&AppendRequest { term: 2, leader: 1, prev_log_index: 2, prev_log_term: 2, entries: vec![Entry { term: 2, cmd: 100 }], leader_commit: 0 }); (r, n.log.len()) }, (AppendReply { term: 2, success: false, match_index: 0 }, 2));
}

#[test]
fn a_conflicting_entry_truncates_the_tail() {
    check!(r#"term 2 replaces a term 1 suffix"#, { let mut n = Node { term: 1, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }, Entry { term: 1, cmd: 2 }, Entry { term: 1, cmd: 3 }], commit_index: 0 }; let r = n.handle_append(&AppendRequest { term: 2, leader: 1, prev_log_index: 1, prev_log_term: 1, entries: vec![Entry { term: 2, cmd: 100 }], leader_commit: 0 }); (r.success, n.log.iter().map(|e| e.term).collect::<Vec<u64>>()) }, (true, vec![1, 2]));
}

#[test]
fn stale_term_is_rejected() {
    check!(r#"term 3 against a term 5 follower"#, { let mut n = Node { term: 5, voted_for: None, log: vec![Entry { term: 1, cmd: 1 }], commit_index: 0 }; let r = n.handle_append(&AppendRequest { term: 3, leader: 1, prev_log_index: 0, prev_log_term: 0, entries: vec![Entry { term: 3, cmd: 100 }], leader_commit: 0 }); (r, n.term, n.log.len()) }, (AppendReply { term: 5, success: false, match_index: 0 }, 5, 1));
}
