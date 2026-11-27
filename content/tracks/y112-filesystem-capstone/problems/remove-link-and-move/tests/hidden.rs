use solution::*;
use std::collections::{BTreeMap, HashMap, HashSet};

const GEOMETRY: Geometry = Geometry { total_blocks: 512, inode_count: 128, journal_blocks: 32 };

fn new_fs() -> Fs<MemDisk> {
    Fs::mkfs(MemDisk::new(512), &GEOMETRY).unwrap()
}

fn links(fs: &mut Fs<MemDisk>, path: &str) -> u16 {
    fs.stat(path).unwrap().nlink
}

fn ino(fs: &mut Fs<MemDisk>, path: &str) -> u32 {
    fs.stat(path).unwrap().ino
}

/// Walks the whole tree and checks that link counts, ".." entries and both bitmaps agree with it.
fn audit(fs: &mut Fs<MemDisk>) -> Result<(), String> {
    let mut seen_dirs = HashSet::from([ROOT]);
    let mut owned: HashMap<u32, u64> = HashMap::new(); // inode -> blocks
    let mut file_names: HashMap<u32, (u16, u16)> = HashMap::new(); // inode -> (stored links, names)
    let mut queue = vec![("/".to_string(), ROOT)];
    while let Some((path, parent)) = queue.pop() {
        let here = fs.stat(&path).map_err(|e| format!("stat {path}: {e}"))?;
        owned.insert(here.ino, here.blocks);
        if fs.lookup(here.ino, "..") != Ok(Some(parent)) || fs.lookup(here.ino, ".") != Ok(Some(here.ino)) {
            return Err(format!("{path}: wrong '.' or '..'"));
        }
        let mut subdirs = 0;
        for (name, n) in fs.readdir(&path).map_err(|e| format!("list {path}: {e}"))? {
            let child = format!("{}/{name}", path.trim_end_matches('/'));
            let stat = fs.stat(&child).map_err(|e| format!("stat {child}: {e}"))?;
            if stat.kind == Kind::Dir {
                subdirs += 1;
                if !seen_dirs.insert(n) {
                    return Err(format!("{child}: a directory with two names"));
                }
                queue.push((child, here.ino));
            } else {
                owned.insert(n, stat.blocks);
                file_names.entry(n).or_insert((stat.nlink, 0)).1 += 1;
            }
        }
        if here.nlink != 2 + subdirs {
            return Err(format!("{path}: {} links with {subdirs} subdirectories", here.nlink));
        }
    }
    if let Some((n, (stored, found))) = file_names.iter().find(|(_, (a, b))| a != b) {
        return Err(format!("inode {n}: {stored} links, {found} names"));
    }
    let data_blocks = fs.superblock().total_blocks - fs.superblock().data_start;
    let used: u64 = owned.values().sum();
    let free = fs.free_blocks().unwrap();
    if free + used != data_blocks {
        return Err(format!("{free} free + {used} owned is not {data_blocks}"));
    }
    let ibm = fs.superblock().ibm_start;
    for n in 1..=fs.superblock().inode_count {
        if fs.bit(ibm, u64::from(n)).unwrap() != owned.contains_key(&n) {
            return Err(format!("inode {n}: bitmap and tree disagree"));
        }
    }
    Ok(())
}

/// /a/b/c and /x, with files /a/f (holding "ff") and /a/b/g, and an empty directory /e.
fn tree() -> Fs<MemDisk> {
    let mut fs = new_fs();
    for dir in ["/a", "/a/b", "/a/b/c", "/x", "/e"] {
        fs.mkdir(dir).unwrap();
    }
    fs.create("/a/f").unwrap();
    fs.write("/a/f", 0, b"ff").unwrap();
    fs.create("/a/b/g").unwrap();
    fs
}

fn kind_of<T>(r: &R<T>) -> Option<std::mem::Discriminant<FsError>> {
    r.as_ref().err().map(std::mem::discriminant)
}

#[test]
fn rename_table() {
    let invalid = FsError::Invalid("");
    for (from, to, want) in [
        ("/a/f", "/x/f", None),
        ("/a/f", "/a/f2", None),
        ("/a/f", "/a/f", None),
        ("/a/f", "/a/b/g", None),
        ("/a/f", "/a/./b/../f", None),
        ("/a/b", "/e", None),
        ("/a/b", "/a/b", None),
        ("/e", "/a/b/c", None),
        ("/a/b/c", "/c", None),
        ("/a/nope", "/x/y", Some(FsError::NotFound)),
        ("/nope/f", "/x/y", Some(FsError::NotFound)),
        ("/a/f", "/nope/y", Some(FsError::NotFound)),
        ("/a/f", "/a/f/y", Some(FsError::NotDir)),
        ("/a/f", "/x", Some(FsError::IsDir)),
        ("/a/b", "/a/f", Some(FsError::NotDir)),
        ("/e", "/a/b", Some(FsError::NotEmpty)),
        ("/a/b/c", "/a", Some(FsError::NotEmpty)),
        ("/a", "/a/y", Some(invalid.clone())),
        ("/a", "/a/b/y", Some(invalid.clone())),
        // the target would also be refused as a non-empty directory: moving into itself is judged first
        ("/a", "/a/b", Some(invalid.clone())),
        // the way to the target passes through the moved directory, whatever the text looks like
        ("/a/b", "/x/../a/b/c/y", Some(invalid.clone())),
        ("/a/b", "/a/b/c/../y", Some(invalid.clone())),
        ("/", "/x/r", Some(invalid.clone())),
        ("/a/f", "/", Some(invalid.clone())),
        ("/a/.", "/y", Some(invalid.clone())),
        ("/a/f", "/x/..", Some(invalid.clone())),
        // names that only look alike
        ("/e", "/a/b/c/e", None),
    ] {
        let mut fs = tree();
        let before = (fs.free_blocks(), fs.readdir("/"), fs.readdir("/a"), fs.readdir("/a/b"));
        let source = fs.stat(from).map(|s| s.ino);
        let got = fs.rename(from, to);
        let want = want.map(|e| std::mem::discriminant(&e));
        check!(format!("rename {from} {to}: {got:?}"), kind_of(&got), want);
        if want.is_none() {
            check!(format!("{from} -> {to}: the same inode is at the new name"), fs.stat(to).map(|s| s.ino), source);
        } else {
            let after = (fs.free_blocks(), fs.readdir("/"), fs.readdir("/a"), fs.readdir("/a/b"));
            check!(format!("{from} -> {to}: nothing changed"), after, before);
        }
        check!(format!("{from} -> {to}: consistent"), audit(&mut fs), Ok(()));
    }
}

#[test]
fn a_prefix_in_the_text_is_not_a_parent() {
    let mut fs = new_fs();
    fs.mkdir("/a").unwrap();
    fs.mkdir("/ab").unwrap();
    check!("/a into /ab", fs.rename("/a", "/ab/a"), Ok(()));
    check!("consistent", audit(&mut fs), Ok(()));
}

#[test]
fn two_names_of_one_file_are_left_alone() {
    let mut fs = tree();
    fs.link("/a/f", "/x/same").unwrap();
    check!("rename onto the other name", fs.rename("/a/f", "/x/same"), Ok(()));
    check!("both names stay", (links(&mut fs, "/a/f"), links(&mut fs, "/x/same")), (2, 2));
    check!("consistent", audit(&mut fs), Ok(()));
}

#[test]
fn replacing_one_of_several_names_keeps_the_file() {
    let mut fs = tree();
    fs.link("/a/b/g", "/x/g2").unwrap();
    fs.write("/x/g2", 0, b"kept").unwrap();
    check!("replace /x/g2", fs.rename("/a/f", "/x/g2"), Ok(()));
    check!("new content at the target", fs.read("/x/g2", 0, 10), Ok(b"ff".to_vec()));
    check!("the old file still has its other name", (fs.read("/a/b/g", 0, 10), links(&mut fs, "/a/b/g")), (Ok(b"kept".to_vec()), 1));
    check!("consistent", audit(&mut fs), Ok(()));
}

#[test]
fn error_order_for_unlink_rmdir_and_link() {
    let mut fs = tree();
    let invalid = || FsError::Invalid("");
    for (path, unlink, rmdir) in [
        ("/", invalid(), invalid()),
        ("/a/.", invalid(), invalid()),
        ("/a/..", invalid(), invalid()),
        ("/nope", FsError::NotFound, FsError::NotFound),
        ("/a/f/x", FsError::NotDir, FsError::NotDir),
        ("rel", invalid(), invalid()),
    ] {
        check!(format!("unlink {path}"), kind_of(&fs.unlink(path)), Some(std::mem::discriminant(&unlink)));
        check!(format!("rmdir {path}"), kind_of(&fs.rmdir(path)), Some(std::mem::discriminant(&rmdir)));
    }
    check!("unlink a directory, even an empty one", fs.unlink("/e"), Err(FsError::IsDir));
    check!("rmdir a file", fs.rmdir("/a/f"), Err(FsError::NotDir));
    check!("link a missing file", fs.link("/nope", "/a/f"), Err(FsError::NotFound));
    check!("link a directory onto a taken name", fs.link("/a", "/x"), Err(FsError::IsDir));
    check!("link onto a taken name", fs.link("/a/f", "/x"), Err(FsError::Exists));
    check!("link onto itself", fs.link("/a/f", "/a/f"), Err(FsError::Exists));
    check!("link into nowhere", fs.link("/a/f", "/nope/y"), Err(FsError::NotFound));
    check!("one link still", links(&mut fs, "/a/f"), 1);
    check!("consistent", audit(&mut fs), Ok(()));
}

#[test]
fn the_link_count_has_a_limit() {
    let mut fs = tree();
    let f = ino(&mut fs, "/a/f");
    let mut inode = fs.load_inode(f).unwrap();
    inode.nlink = u16::MAX;
    fs.tx(|fs| fs.store_inode(f, &inode)).unwrap();
    check!("one more", matches!(fs.link("/a/f", "/x/more"), Err(FsError::Invalid(_))), true);
    check!("no new name", fs.stat("/x/more"), Err(FsError::NotFound));
}

#[test]
fn removing_a_large_sparse_file_frees_every_block() {
    let mut fs = new_fs();
    fs.create("/f").unwrap();
    let free = fs.free_blocks().unwrap();
    for block in [0, 11, 12, 1035, 1036, 2060, MAX_FILE_BLOCKS - 1] {
        fs.write("/f", block * BS as u64, b"z").unwrap();
    }
    fs.create("/g").unwrap();
    check!("rename over it", fs.rename("/g", "/f"), Ok(()));
    check!("free again", fs.free_blocks(), Ok(free));
    check!("consistent", audit(&mut fs), Ok(()));
}

#[test]
fn names_and_blocks_can_be_used_for_ever() {
    let g = Geometry { total_blocks: 48, inode_count: 8, journal_blocks: 12 };
    let mut fs = Fs::mkfs(MemDisk::new(48), &g).unwrap();
    for round in 0..60 {
        let what = format!("round {round}");
        check!(what.clone(), fs.mkdir("/d").is_ok() && fs.create("/d/f").is_ok(), true);
        check!(what.clone(), fs.write("/d/f", 0, &vec![1; 15 * BS]), Ok(()));
        check!(what.clone(), fs.link("/d/f", "/h"), Ok(()));
        check!(what.clone(), (fs.unlink("/d/f"), fs.rmdir("/d"), fs.unlink("/h")), (Ok(()), Ok(()), Ok(())));
    }
}

#[test]
fn a_loop_of_parent_entries_is_an_error() {
    let mut fs = tree();
    let c = ino(&mut fs, "/a/b/c");
    fs.tx(|fs| fs.set_entry(c, "..", c)).unwrap();
    let got = fs.rename("/x", "/a/b/c/x");
    check!("'..' points at itself", matches!(got, Err(FsError::Corrupt(_))), true);
    let b = ino(&mut fs, "/a/b");
    fs.tx(|fs| {
        fs.set_entry(c, "..", b)?;
        fs.set_entry(b, "..", c)
    })
    .unwrap();
    let got = fs.rename("/x", "/a/b/c/x");
    check!("two directories point at each other", matches!(got, Err(FsError::Corrupt(_))), true);
    fs.tx(|fs| fs.remove_entry(b, "..")).unwrap();
    let got = fs.rename("/x", "/a/b/c/x");
    check!("'..' is missing", matches!(got, Err(FsError::Corrupt(_))), true);
    check!("a file may still move there", fs.rename("/a/f", "/a/b/c/f"), Ok(()));
}

/// The model: paths to nodes, and file contents by number so that two names can share one.
#[derive(Default)]
struct Model {
    nodes: BTreeMap<String, Option<usize>>, // None: a directory
    files: Vec<Vec<u8>>,
}

impl Model {
    fn parent_is_dir(&self, path: &str) -> bool {
        let parent = &path[..path.rfind('/').unwrap()];
        parent.is_empty() || self.nodes.get(parent) == Some(&None)
    }
    fn empty(&self, path: &str) -> bool {
        let prefix = format!("{path}/");
        !self.nodes.keys().any(|k| k.starts_with(&prefix))
    }
    fn rename(&mut self, from: &str, to: &str) -> bool {
        let Some(source) = self.nodes.get(from).copied() else { return false };
        if !self.parent_is_dir(to) || (source.is_none() && to.starts_with(&format!("{from}/"))) {
            return false;
        }
        if from == to {
            return true;
        }
        match (source, self.nodes.get(to).copied()) {
            (Some(a), Some(Some(b))) if a == b => return true,
            (Some(_), Some(Some(_))) | (_, None) => {}
            (None, Some(None)) if self.empty(to) => {}
            _ => return false,
        }
        let prefix = format!("{from}/");
        let moved: Vec<String> = self.nodes.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
        self.nodes.remove(from);
        self.nodes.insert(to.to_string(), source);
        for old in moved {
            let node = self.nodes.remove(&old).unwrap();
            self.nodes.insert(format!("{to}/{}", &old[prefix.len()..]), node);
        }
        true
    }
}

#[test]
fn random_operations_match_a_map_of_paths() {
    let mut rng = anneal_prelude::Rng::new(110_601);
    for round in 0..10 {
        let mut fs = new_fs();
        let mut model = Model::default();
        for step in 0..250 {
            let path = |rng: &mut anneal_prelude::Rng, model: &Model| -> String {
                if !model.nodes.is_empty() && rng.bool() {
                    model.nodes.keys().nth(rng.below(model.nodes.len())).unwrap().clone()
                } else {
                    (0..1 + rng.below(3)).map(|_| format!("/{}", ["a", "b", "c"][rng.below(3)])).collect()
                }
            };
            let (p, q) = (path(&mut rng, &model), path(&mut rng, &model));
            let node = model.nodes.get(&p).copied();
            let free = |m: &Model, p: &str| !m.nodes.contains_key(p) && m.parent_is_dir(p);
            let (what, got, want) = match rng.below(7) {
                0 => {
                    let ok = free(&model, &p);
                    if ok {
                        model.nodes.insert(p.clone(), None);
                    }
                    (format!("mkdir {p}"), fs.mkdir(&p).is_ok(), ok)
                }
                1 => {
                    let ok = free(&model, &p);
                    if ok {
                        model.files.push(Vec::new());
                        model.nodes.insert(p.clone(), Some(model.files.len() - 1));
                    }
                    (format!("create {p}"), fs.create(&p).is_ok(), ok)
                }
                2 => {
                    let byte = rng.below(255) as u8 + 1;
                    let len = rng.below(6000);
                    if let Some(Some(f)) = node {
                        model.files[f] = vec![byte; len];
                    }
                    let got = fs.truncate(&p, 0).and_then(|()| fs.write(&p, 0, &vec![byte; len]));
                    (format!("rewrite {p}"), got.is_ok(), matches!(node, Some(Some(_))))
                }
                3 => {
                    let ok = matches!(node, Some(Some(_)));
                    if ok {
                        model.nodes.remove(&p);
                    }
                    (format!("unlink {p}"), fs.unlink(&p).is_ok(), ok)
                }
                4 => {
                    let ok = node == Some(None) && model.empty(&p);
                    if ok {
                        model.nodes.remove(&p);
                    }
                    (format!("rmdir {p}"), fs.rmdir(&p).is_ok(), ok)
                }
                5 => {
                    let ok = matches!(node, Some(Some(_))) && free(&model, &q);
                    if ok {
                        model.nodes.insert(q.clone(), node.unwrap());
                    }
                    (format!("link {p} {q}"), fs.link(&p, &q).is_ok(), ok)
                }
                _ => {
                    let ok = model.rename(&p, &q);
                    (format!("rename {p} {q}"), fs.rename(&p, &q).is_ok(), ok)
                }
            };
            check!(format!("round {round} step {step}: {what} succeeds"), got, want);
            if step % 50 == 49 {
                check!(format!("round {round} step {step}: consistent"), audit(&mut fs), Ok(()));
            }
        }
        let mut fs = Fs::mount(fs.into_device()).unwrap();
        check!(format!("round {round}: consistent after a remount"), audit(&mut fs), Ok(()));
        let mut paths = Vec::new();
        let mut queue = vec![String::new()];
        while let Some(dir) = queue.pop() {
            for (name, _) in fs.readdir(&format!("{dir}/")).unwrap() {
                let child = format!("{dir}/{name}");
                if fs.stat(&child).unwrap().kind == Kind::Dir {
                    queue.push(child.clone());
                }
                paths.push(child);
            }
        }
        paths.sort();
        check!(format!("round {round}: paths"), paths, model.nodes.keys().cloned().collect::<Vec<_>>());
        for (path, node) in &model.nodes {
            if let Some(f) = node {
                let names = model.nodes.values().filter(|n| **n == Some(*f)).count();
                check!(format!("round {round}: {path}"), (fs.read(path, 0, 10_000).unwrap() == model.files[*f], usize::from(links(&mut fs, path))), (true, names));
            }
        }
    }
}
