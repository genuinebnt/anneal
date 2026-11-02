use solution::*;

#[test]
fn majority_of_three() {
    check!(r#"leader + one follower is enough"#, advance_commit(&[1, 1, 1], 1, &[3, 2], 0), 3);
}

#[test]
fn partial_replication() {
    check!(r#"index 1 has a majority, 2 and 3 do not"#, advance_commit(&[1, 1, 1], 1, &[1, 0], 0), 1);
}

#[test]
fn no_majority_keeps_the_commit_index() {
    check!(r#"only the leader has it"#, advance_commit(&[1, 1], 1, &[0, 0], 0), 0);
}

#[test]
fn five_servers_need_three() {
    check!(r#"two copies is not enough, three is"#, (advance_commit(&[1, 1, 1], 1, &[3, 0, 0, 0], 0), advance_commit(&[1, 1, 1], 1, &[3, 3, 0, 0], 0)), (0, 3));
}

#[test]
fn old_term_entries_are_not_counted() {
    check!(r#"Figure 8: entry 3 is from term 2"#, advance_commit(&[1, 1, 2], 3, &[3, 3], 0), 0);
}

#[test]
fn a_current_term_entry_commits_what_is_below_it() {
    check!(r#"index 4 is term 3"#, advance_commit(&[1, 1, 2, 3], 3, &[4, 4], 0), 4);
}
