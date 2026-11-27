use solution::*;
use std::collections::BTreeMap;

fn tree(paths: &[&str]) -> Tree {
    let mut tree = Tree::new();
    for path in paths {
        tree.make(path).unwrap();
    }
    tree
}

/// Every folder: path -> number.
fn all(t: &Tree) -> BTreeMap<String, FolderId> {
    let mut found = BTreeMap::new();
    let mut queue = vec![String::new()];
    while let Some(path) = queue.pop() {
        for name in t.list(&format!("{path}/")).unwrap() {
            let child = format!("{path}/{name}");
            found.insert(child.clone(), t.find(&child).unwrap());
            queue.push(child);
        }
    }
    found
}

#[test]
fn the_way_may_go_through_the_moved_folder() {
    for (from, to) in [("/a/b", "/x/../a/b/c/new"), ("/a/b", "/a/b/c/../new"), ("/a", "/a/b/../b/new"), ("/a/b/c/..", "/x/y")] {
        let mut t = tree(&["/a", "/a/b", "/a/b/c", "/x"]);
        let before = all(&t);
        let want = if from.ends_with("..") { TreeError::BadPath } else { TreeError::IntoItself };
        check!(format!("{from} -> {to}"), t.rename(from, to), Err(want));
        check!(format!("{from} -> {to}: unchanged"), all(&t), before);
    }
}

#[test]
fn a_way_that_only_passes_through_is_fine() {
    let mut t = tree(&["/a", "/a/b", "/x"]);
    let b = t.find("/a/b").unwrap();
    // the text goes through /a/b, but the new parent is /x
    check!("rename", t.rename("/a/b", "/a/b/../../x/b"), Ok(()));
    check!("moved", t.find("/x/b"), Ok(b));
}

#[test]
fn dots_do_not_hide_missing_folders() {
    let t = tree(&["/a"]);
    check!("'..' after a missing part", t.find("/nope/../a"), Err(TreeError::NotFound));
    check!("'.' after a missing part", t.find("/nope/."), Err(TreeError::NotFound));
    check!("'..' of the top", t.find("/../../a"), t.find("/a"));
    check!("extra slashes", t.find("//a///"), t.find("/a"));
}

#[test]
fn bad_paths() {
    let mut t = tree(&["/a", "/a/b"]);
    for bad in ["", "a", "a/b", "./a"] {
        check!(format!("find {bad:?}"), t.find(bad), Err(TreeError::BadPath));
        check!(format!("list {bad:?}"), t.list(bad), Err(TreeError::BadPath));
    }
    // the top and the dots cannot be made, removed or moved
    for bad in ["/", "//", "/a/.", "/a/..", "/a/b/..", "a"] {
        check!(format!("make {bad:?}"), t.make(bad), Err(TreeError::BadPath));
        check!(format!("remove {bad:?}"), t.remove(bad), Err(TreeError::BadPath));
        check!(format!("move {bad:?}"), t.rename(bad, "/z"), Err(TreeError::BadPath));
        check!(format!("move onto {bad:?}"), t.rename("/a/b", bad), Err(TreeError::BadPath));
    }
}

#[test]
fn rename_errors_in_order() {
    let mut t = tree(&["/a", "/a/b", "/x", "/x/y"]);
    check!("missing source before a bad target", t.rename("/no", "rel"), Err(TreeError::NotFound));
    check!("bad source first", t.rename("rel", "/no/z"), Err(TreeError::BadPath));
    check!("missing target parent", t.rename("/a", "/no/z"), Err(TreeError::NotFound));
    check!("taken", t.rename("/a", "/x"), Err(TreeError::Exists));
    check!("taken deeper", t.rename("/a/b", "/x/y"), Err(TreeError::Exists));
    check!("into itself before taken", t.rename("/a", "/a/b"), Err(TreeError::IntoItself));
    check!("onto its own path", t.rename("/a/b", "/a/b"), Ok(()));
    check!("onto its own path, written another way", t.rename("/a/b", "/x/../a/./b"), Ok(()));
    check!("up to a parent's parent", t.rename("/a/b", "/b"), Ok(()));
}

#[test]
fn numbers_are_never_used_again() {
    let mut t = Tree::new();
    let first = t.make("/a").unwrap();
    t.remove("/a").unwrap();
    let second = t.make("/a").unwrap();
    check!("a new number", first != second && first != TOP && second != TOP, true);
    check!("the old one leads nowhere", t.path_of(first), None);
    check!("unknown number", t.path_of(1_000_000), None);
    check!("the top", t.path_of(TOP), Some("/".to_string()));
}

#[test]
fn removed_means_gone_for_paths_too() {
    let mut t = tree(&["/a", "/a/b"]);
    t.remove("/a/b").unwrap();
    check!("find", t.find("/a/b/.."), Err(TreeError::NotFound));
    check!("remove again", t.remove("/a/b"), Err(TreeError::NotFound));
    check!("list", t.list("/a"), Ok(vec![]));
}

#[test]
fn a_deep_chain() {
    let mut t = Tree::new();
    let mut path = String::new();
    for _ in 0..1500 {
        path.push_str("/d");
        t.make(&path).unwrap();
    }
    let last = t.find(&path).unwrap();
    check!("path", t.path_of(last) == Some(path.clone()), true);
    check!("the top of the chain into its end", t.rename("/d", &format!("{path}/again")), Err(TreeError::IntoItself));
}

#[test]
fn random_operations_match_a_set_of_paths() {
    let mut rng = anneal_prelude::Rng::new(111_401);
    for round in 0..60 {
        let mut t = Tree::new();
        let mut model: BTreeMap<String, FolderId> = BTreeMap::new();
        for step in 0..120 {
            let path = |rng: &mut anneal_prelude::Rng, model: &BTreeMap<String, FolderId>| -> String {
                if !model.is_empty() && rng.bool() {
                    model.keys().nth(rng.below(model.len())).unwrap().clone()
                } else {
                    (0..1 + rng.below(3)).map(|_| format!("/{}", ["a", "b", "c"][rng.below(3)])).collect()
                }
            };
            let (p, q) = (path(&mut rng, &model), path(&mut rng, &model));
            let parent_there = |m: &BTreeMap<String, FolderId>, p: &str| {
                let up = &p[..p.rfind('/').unwrap()];
                up.is_empty() || m.contains_key(up)
            };
            let below = |p: &str| format!("{p}/");
            let what = format!("round {round} step {step}");
            match rng.below(3) {
                0 => {
                    let want = if !parent_there(&model, &p) {
                        Err(TreeError::NotFound)
                    } else if model.contains_key(&p) {
                        Err(TreeError::Exists)
                    } else {
                        Ok(())
                    };
                    let got = t.make(&p);
                    check!(format!("{what}: make {p}"), got.map(|_| ()), want);
                    if let Ok(id) = got {
                        check!(format!("{what}: the number is new"), id != TOP && !model.values().any(|v| *v == id), true);
                        model.insert(p, id);
                    }
                }
                1 => {
                    let want = if !model.contains_key(&p) {
                        Err(TreeError::NotFound)
                    } else if model.keys().any(|k| k.starts_with(&below(&p))) {
                        Err(TreeError::NotEmpty)
                    } else {
                        Ok(())
                    };
                    check!(format!("{what}: remove {p}"), t.remove(&p), want);
                    if want.is_ok() {
                        model.remove(&p);
                    }
                }
                _ => {
                    let want = if !model.contains_key(&p) || !parent_there(&model, &q) {
                        Err(TreeError::NotFound)
                    } else if q.starts_with(&below(&p)) {
                        Err(TreeError::IntoItself)
                    } else if p != q && model.contains_key(&q) {
                        Err(TreeError::Exists)
                    } else {
                        Ok(())
                    };
                    check!(format!("{what}: rename {p} {q}"), t.rename(&p, &q), want);
                    if want.is_ok() {
                        let moved: Vec<String> = model.keys().filter(|k| **k == p || k.starts_with(&below(&p))).cloned().collect();
                        let taken: Vec<(String, FolderId)> = moved.iter().map(|k| (format!("{q}{}", &k[p.len()..]), model.remove(k).unwrap())).collect();
                        model.extend(taken);
                    }
                }
            }
            check!(format!("{what}: all folders"), all(&t), model.clone());
            if let Some((path, id)) = model.iter().nth(step % model.len().max(1)) {
                check!(format!("{what}: path_of"), t.path_of(*id), Some(path.clone()));
            }
        }
    }
}
