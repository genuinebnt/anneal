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
fn unlink_gives_everything_back() {
    let mut fs = new_fs();
    let free = fs.free_blocks().unwrap();
    fs.create("/f").unwrap();
    fs.write("/f", 0, &vec![1; 20 * BS]).unwrap();
    check!("unlink", fs.unlink("/f"), Ok(()));
    check!("gone", fs.stat("/f"), Err(FsError::NotFound));
    check!("the tree is consistent", audit(&mut fs), Ok(()));
    check!("free blocks (the root keeps its one)", fs.free_blocks(), Ok(free));
}

#[test]
fn a_file_lives_while_it_has_a_name() {
    let mut fs = tree();
    check!("link", fs.link("/a/f", "/x/h"), Ok(()));
    check!("two links, one inode", (links(&mut fs, "/x/h"), ino(&mut fs, "/x/h") == ino(&mut fs, "/a/f")), (2, true));
    fs.write("/x/h", 1, b"X").unwrap();
    check!("unlink the first name", fs.unlink("/a/f"), Ok(()));
    check!("still there", fs.read("/x/h", 0, 10), Ok(b"fX".to_vec()));
    check!("one link", links(&mut fs, "/x/h"), 1);
    check!("consistent", audit(&mut fs), Ok(()));
}

#[test]
fn rmdir_wants_an_empty_directory() {
    let mut fs = tree();
    check!("not empty", fs.rmdir("/a/b"), Err(FsError::NotEmpty));
    check!("empty", fs.rmdir("/a/b/c"), Ok(()));
    check!("the parent lost a link", links(&mut fs, "/a/b"), 2);
    check!("consistent", audit(&mut fs), Ok(()));
}

#[test]
fn moving_a_directory_takes_its_contents() {
    let mut fs = tree();
    check!("rename", fs.rename("/a/b", "/x/b2"), Ok(()));
    check!("old name", fs.stat("/a/b"), Err(FsError::NotFound));
    check!("'..' follows", fs.resolve("/x/b2/.."), fs.resolve("/x"));
    check!("links of /a and /x", (links(&mut fs, "/a"), links(&mut fs, "/x")), (2, 3));
    check!("consistent", audit(&mut fs), Ok(()));
}

#[test]
fn a_directory_cannot_move_into_itself() {
    let mut fs = tree();
    let got = fs.rename("/a", "/a/b/c/a");
    check!("invalid", matches!(got, Err(FsError::Invalid(_))), true);
    check!("still there", fs.resolve("/a/b/c").is_ok(), true);
}
