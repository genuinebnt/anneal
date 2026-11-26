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
fn directory_changes_arrive_in_order() {
    // "b" can only be there if "a" is
    let ops = [create("a"), create("b")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[]), dir(&[("a", b"")]), dir(&[("a", b""), ("b", b"")])]));
}

#[test]
fn each_file_is_cut_on_its_own() {
    let ops = [create("a"), create("b"), Op::SyncDir, write("a", b"1"), write("b", b"2")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[
        dir(&[("a", b""), ("b", b"")]), dir(&[("a", b"1"), ("b", b"")]),
        dir(&[("a", b""), ("b", b"2")]), dir(&[("a", b"1"), ("b", b"2")]),
    ]));
}

#[test]
fn bytes_synced_earlier_stay() {
    let ops = [create("a"), Op::SyncDir, write("a", b"ab"), sync("a"), write("a", b"c"), write("a", b"d")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"ab")]), dir(&[("a", b"abc")]), dir(&[("a", b"abcd")])]));
}

#[test]
fn syncing_one_file_leaves_the_other() {
    let ops = [create("a"), create("b"), Op::SyncDir, write("a", b"1"), write("b", b"2"), sync("a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"1"), ("b", b"")]), dir(&[("a", b"1"), ("b", b"2")])]));
}

#[test]
fn an_unsynced_remove_may_come_back() {
    let ops = [create("a"), write("a", b"k"), sync("a"), Op::SyncDir, remove("a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[]), dir(&[("a", b"k")])]));
    let mut done = ops.to_vec();
    done.push(Op::SyncDir);
    check!("and after a directory sync", after_power_cut(&done), set(&[dir(&[])]));
}

#[test]
fn create_over_a_name_is_a_new_file() {
    let ops = [create("a"), write("a", b"old"), sync("a"), Op::SyncDir, create("a"), write("a", b"n")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"old")]), dir(&[("a", b"")]), dir(&[("a", b"n")])]));
}

#[test]
fn a_sync_follows_the_file_through_a_rename() {
    let ops = [create("t"), write("t", b"z"), Op::SyncDir, rename("t", "a"), sync("a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("t", b"z")]), dir(&[("a", b"z")])]));
}

#[test]
fn writes_after_a_rename_go_to_the_same_file() {
    let ops = [create("a"), Op::SyncDir, rename("a", "b"), write("b", b"q")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[
        dir(&[("a", b"")]), dir(&[("a", b"q")]), dir(&[("b", b"")]), dir(&[("b", b"q")]),
    ]));
}

#[test]
fn the_careful_replacement_is_old_or_new() {
    let ops = [create("s"), write("s", b"old"), sync("s"), Op::SyncDir, create("t"), write("t", b"new"), sync("t"), rename("t", "s")];
    for state in after_power_cut(&ops) {
        let s = state.get("s").cloned();
        check!(format!("a state: {state:?}"), s == Some(b"old".to_vec()) || s == Some(b"new".to_vec()), true);
    }
}

/// A second way to the same answer: for one choice of how many directory changes arrived, replay the whole history and
/// follow each file by number.
fn by_replay(ops: &[Op], arrived: usize) -> Option<Vec<(String, Vec<u8>, usize)>> {
    let last_sync = ops.iter().rposition(|o| *o == Op::SyncDir).map_or(0, |i| i + 1);
    let mut live: std::collections::BTreeMap<String, usize> = Default::default();
    let mut disk = live.clone();
    let mut files: Vec<(Vec<u8>, usize)> = Vec::new();
    let mut left = arrived;
    for (i, op) in ops.iter().enumerate() {
        let changes_names = matches!(op, Op::Create(_) | Op::Rename(..) | Op::Remove(_));
        let reaches_disk = changes_names && (i < last_sync || left > 0);
        if changes_names && i >= last_sync && left > 0 {
            left -= 1;
        }
        let targets: Vec<&mut std::collections::BTreeMap<String, usize>> = if reaches_disk { vec![&mut live, &mut disk] } else { vec![&mut live] };
        match op {
            Op::Create(n) => {
                files.push((Vec::new(), 0));
                for d in targets {
                    d.insert(n.clone(), files.len() - 1);
                }
            }
            Op::Rename(a, b) => {
                for d in targets {
                    if let Some(f) = d.remove(a) {
                        d.insert(b.clone(), f);
                    }
                }
            }
            Op::Remove(n) => {
                for d in targets {
                    d.remove(n);
                }
            }
            Op::Write(n, bytes) => files[live[n]].0.extend_from_slice(bytes),
            Op::SyncFile(n) => {
                let f = &mut files[live[n]];
                f.1 = f.0.len();
            }
            Op::SyncDir => {}
        }
    }
    (left == 0).then(|| disk.into_iter().map(|(n, f)| (n, files[f].0.clone(), files[f].1)).collect())
}

#[test]
fn random_histories() {
    let mut rng = anneal_prelude::Rng::new(170201);
    for _ in 0..1500 {
        let mut ops = Vec::new();
        let mut names: BTreeSet<String> = BTreeSet::new();
        let steps = 1 + rng.below(8);
        for _ in 0..steps {
            let any = ["a", "b", "c"][rng.below(3)].to_string();
            let existing: Vec<String> = names.iter().cloned().collect();
            let pick = if existing.is_empty() { None } else { Some(existing[rng.below(existing.len())].clone()) };
            match (rng.below(7), pick) {
                (0, _) | (_, None) => {
                    names.insert(any.clone());
                    ops.push(Op::Create(any));
                }
                (1 | 2, Some(n)) => {
                    let len = 1 + rng.below(2);
                    ops.push(Op::Write(n, rng.vec(len, 0, 3)));
                }
                (3, Some(n)) => ops.push(Op::SyncFile(n)),
                (4, Some(n)) => {
                    names.remove(&n);
                    names.insert(any.clone());
                    ops.push(Op::Rename(n, any));
                }
                (5, Some(n)) => {
                    names.remove(&n);
                    ops.push(Op::Remove(n));
                }
                _ => ops.push(Op::SyncDir),
            }
        }
        let mut want: BTreeSet<Files> = BTreeSet::new();
        for arrived in 0..=ops.len() {
            let Some(files) = by_replay(&ops, arrived) else { continue };
            let mut states = vec![Files::new()];
            for (name, data, synced) in files {
                states = states
                    .iter()
                    .flat_map(|s| {
                        (synced..=data.len()).map(|k| {
                            let mut s = s.clone();
                            s.insert(name.clone(), data[..k].to_vec());
                            s
                        })
                    })
                    .collect();
            }
            want.extend(states);
        }
        check!(format!("{ops:?}"), after_power_cut(&ops), want);
    }
}
