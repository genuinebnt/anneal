use solution::*;

use std::collections::BTreeSet;

#[allow(dead_code)]
fn create(n: &str) -> Op {
    Op::Create(n.into())
}
#[allow(dead_code)]
fn write(n: &str, b: &[u8]) -> Op {
    Op::Write(n.into(), b.to_vec())
}
#[allow(dead_code)]
fn sync(n: &str) -> Op {
    Op::SyncFile(n.into())
}
#[allow(dead_code)]
fn rename(a: &str, b: &str) -> Op {
    Op::Rename(a.into(), b.into())
}
#[allow(dead_code)]
fn remove(n: &str) -> Op {
    Op::Remove(n.into())
}
#[allow(dead_code)]
fn dir(files: &[(&str, &[u8])]) -> Files {
    files.iter().map(|(n, b)| (n.to_string(), b.to_vec())).collect()
}
#[allow(dead_code)]
fn set(dirs: &[Files]) -> BTreeSet<Files> {
    dirs.iter().cloned().collect()
}

#[test]
fn nothing_done_nothing_there() {
    check!("[]", after_power_cut(&[]), set(&[dir(&[])]));
}

#[test]
fn unsynced_bytes_may_be_cut_anywhere() {
    let ops = [create("a"), Op::SyncDir, write("a", b"hi")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"")]), dir(&[("a", b"h")]), dir(&[("a", b"hi")])]));
}

#[test]
fn syncing_a_file_does_not_save_its_name() {
    let ops = [create("a"), write("a", b"x"), sync("a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[]), dir(&[("a", b"x")])]));
}

#[test]
fn syncing_the_directory_does_not_save_bytes() {
    let ops = [create("a"), write("a", b"x"), Op::SyncDir];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"")]), dir(&[("a", b"x")])]));
}

#[test]
fn a_rename_can_reach_the_disk_before_the_bytes_do() {
    let ops = [create("t"), write("t", b"v2"), rename("t", "a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[
        dir(&[]),
        dir(&[("t", b"")]), dir(&[("t", b"v")]), dir(&[("t", b"v2")]),
        dir(&[("a", b"")]), dir(&[("a", b"v")]), dir(&[("a", b"v2")]),
    ]));
}
