use solution::*;

#[test]
fn four_servers_need_three() {
    check!(r#"2 of 4 is not a majority"#, (advance_commit(&[1, 1], 1, &[2, 0, 0], 0), advance_commit(&[1, 1], 1, &[2, 2, 0], 0)), (0, 2));
}

#[test]
fn a_single_node_cluster_commits_alone() {
    check!(r#"no followers"#, advance_commit(&[1, 1], 1, &[], 0), 2);
}

#[test]
fn an_empty_log_commits_nothing() {
    check!(r#"no entries"#, advance_commit(&[], 1, &[0, 0], 0), 0);
}

#[test]
fn the_largest_qualifying_index_wins() {
    check!(r#"not the first one"#, advance_commit(&[1, 1, 1, 1], 1, &[4, 2], 0), 4);
}

#[test]
fn never_goes_backwards() {
    check!(r#"already committed past what the followers report"#, (advance_commit(&[1, 1, 1], 1, &[1, 1], 3), advance_commit(&[1, 1, 1], 1, &[3, 3], 2)), (3, 3));
}

#[test]
fn stops_where_replication_stops() {
    check!(r#"indexes 2 and 3 are on a majority, 4 is not"#, advance_commit(&[1, 3, 3, 3], 3, &[3, 1], 0), 3);
}

#[test]
fn old_entries_in_the_middle_are_skipped_but_not_blocking() {
    check!(r#"term 3 entries after term 1"#, advance_commit(&[1, 3, 3], 3, &[3, 3], 0), 3);
}

#[test]
fn only_old_entries_replicated() {
    check!(r#"the new term has none yet"#, advance_commit(&[1, 2, 2], 3, &[3, 3, 3, 3], 1), 1);
}

#[test]
fn the_leader_counts_as_a_replica() {
    check!(r#"one follower of three is a majority with the leader"#, advance_commit(&[2], 2, &[1, 0], 0), 1);
}

#[test]
fn future_terms_are_never_current() {
    check!(r#"terms above current_term do not count either"#, advance_commit(&[2, 5], 2, &[2, 2], 0), 1);
}
