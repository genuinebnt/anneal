use solution::*;

fn geometry(total_blocks: u64, inode_count: u32, journal_blocks: u32) -> Geometry {
    Geometry { total_blocks, inode_count, journal_blocks }
}

fn new_fs() -> Fs<MemDisk> {
    Fs::mkfs(MemDisk::new(256), &geometry(256, 64, 32)).unwrap()
}

fn names(fs: &mut Fs<MemDisk>, path: &str) -> Vec<String> {
    fs.readdir(path).unwrap().into_iter().map(|(n, _)| n).collect()
}

/// An entry as it is stored: inode, name length, name.
fn raw_entry(ino: u32, name: &[u8]) -> [u8; 64] {
    let mut e = [0u8; 64];
    e[..4].copy_from_slice(&ino.to_le_bytes());
    e[4] = name.len() as u8;
    e[5..5 + name.len()].copy_from_slice(name);
    e
}

/// Writes over part of a directory's stored bytes.
fn poke(fs: &mut Fs<MemDisk>, dir: u32, offset: u64, bytes: &[u8]) {
    fs.tx(|fs| {
        let mut inode = fs.load_inode(dir)?;
        fs.write_at(dir, &mut inode, offset, bytes)
    })
    .unwrap();
}

#[test]
fn errors_for_new_names() {
    let mut fs = new_fs();
    fs.mkdir("/d").unwrap();
    fs.create("/f").unwrap();
    let long = format!("/{}", "x".repeat(60));
    let longest = format!("/{}", "x".repeat(59));
    for (path, want) in [
        ("f2", Some(FsError::Invalid(""))),
        ("", Some(FsError::Invalid(""))),
        ("/", Some(FsError::Invalid(""))),
        ("//", Some(FsError::Invalid(""))),
        ("/d/.", Some(FsError::Invalid(""))),
        ("/d/..", Some(FsError::Invalid(""))),
        ("/a\0b", Some(FsError::Invalid(""))),
        // the name is judged before the directories on the way
        ("/nope/..", Some(FsError::Invalid(""))),
        (long.as_str(), Some(FsError::NameTooLong)),
        ("/nope/x", Some(FsError::NotFound)),
        ("/f/x", Some(FsError::NotDir)),
        ("/f/x/y", Some(FsError::NotDir)),
        ("/d", Some(FsError::Exists)),
        ("/f", Some(FsError::Exists)),
        (longest.as_str(), None),
    ] {
        let kind = |r: R<u32>| r.err().map(|e| std::mem::discriminant(&e));
        let want = want.map(|e| std::mem::discriminant(&e));
        let free = fs.free_blocks().unwrap();
        let before = names(&mut fs, "/");
        check!(format!("create {path:?}"), kind(fs.create(path)), want);
        if want.is_some() {
            check!(format!("after create {path:?}"), (fs.free_blocks().unwrap(), names(&mut fs, "/")), (free, before));
            check!(format!("mkdir {path:?}"), kind(fs.mkdir(path)), want);
        }
    }
}

#[test]
fn errors_for_old_names() {
    let mut fs = new_fs();
    fs.mkdir("/d").unwrap();
    fs.create("/f").unwrap();
    check!("read a directory", fs.read("/d", 0, 10), Err(FsError::IsDir));
    check!("write a directory", fs.write("/d", 0, b"x"), Err(FsError::IsDir));
    check!("truncate a directory", fs.truncate("/d", 0), Err(FsError::IsDir));
    check!("write nothing to a missing file", fs.write("/no", 0, b""), Err(FsError::NotFound));
    check!("read missing", fs.read("/no", 0, 1), Err(FsError::NotFound));
    check!("through a file", fs.stat("/f/x"), Err(FsError::NotDir));
    check!("'..' of a file", fs.stat("/f/.."), Err(FsError::NotDir));
    check!("list a file", fs.readdir("/f"), Err(FsError::NotDir));
    check!("relative", matches!(fs.stat("d"), Err(FsError::Invalid(_))), true);
    check!("truncate too far", fs.truncate("/f", MAX_FILE_SIZE + 1), Err(FsError::FileTooBig));
    check!("write too far", fs.write("/f", MAX_FILE_SIZE - 1, b"ab"), Err(FsError::FileTooBig));
    check!("write past u64", fs.write("/f", u64::MAX, b"ab"), Err(FsError::FileTooBig));
    check!("size unchanged", fs.stat("/f").map(|s| s.size), Ok(0));
}

#[test]
fn slashes_and_dots_in_paths() {
    let mut fs = new_fs();
    fs.mkdir("/a/").unwrap();
    fs.mkdir("//a///b").unwrap();
    let b = fs.resolve("/a/b").unwrap();
    for path in ["/a/b/", "//a//b", "/./a/./b/.", "/a/b/../b", "/../../a/b"] {
        check!(path, fs.resolve(path), Ok(b));
    }
    check!("the root's parent is the root", fs.resolve("/.."), Ok(ROOT));
}

#[test]
fn unicode_names_are_counted_in_bytes() {
    let mut fs = new_fs();
    let fits = format!("/{}", "é".repeat(29)); // 58 bytes
    let over = format!("/{}", "é".repeat(30)); // 60 bytes
    check!("58 bytes", fs.create(&fits).is_ok(), true);
    check!("60 bytes", fs.create(&over), Err(FsError::NameTooLong));
    check!("listed", names(&mut fs, "/"), vec!["é".repeat(29)]);
}

#[test]
fn entries_are_stored_as_the_table_says() {
    let mut fs = new_fs();
    let d = fs.mkdir("/d").unwrap();
    let f = fs.create("/d/file").unwrap();
    let inode = fs.load_inode(d).unwrap();
    let raw = fs.read_at(&inode, 0, 1000).unwrap();
    let mut want = raw_entry(d, b".").to_vec();
    want.extend_from_slice(&raw_entry(ROOT, b".."));
    want.extend_from_slice(&raw_entry(f, b"file"));
    check!("the directory's bytes", raw, want);
}

#[test]
fn a_cleared_slot_is_used_again() {
    let mut fs = new_fs();
    for name in ["/a", "/b", "/c"] {
        fs.create(name).unwrap();
    }
    let size = fs.stat("/").unwrap().size;
    fs.tx(|fs| fs.remove_entry(ROOT, "a")).unwrap();
    check!("a is gone", fs.resolve("/a"), Err(FsError::NotFound));
    check!("the directory keeps its size", fs.stat("/").map(|s| s.size), Ok(size));
    fs.tx(|fs| fs.add_entry(ROOT, "z", 9)).unwrap();
    check!("z took the first free slot", fs.stat("/").map(|s| s.size), Ok(size));
    let root = fs.load_inode(ROOT).unwrap();
    check!("slot 2", fs.read_at(&root, 128, 64), Ok(raw_entry(9, b"z").to_vec()));
    check!("remove what is not there", fs.tx(|fs| fs.remove_entry(ROOT, "a")), Err(FsError::NotFound));
    check!("point z elsewhere", fs.tx(|fs| fs.set_entry(ROOT, "z", 7)), Ok(()));
    check!("lookup", fs.lookup(ROOT, "z"), Ok(Some(7)));
    check!("point nothing elsewhere", fs.tx(|fs| fs.set_entry(ROOT, "a", 7)), Err(FsError::NotFound));
    for bad in ["", &"y".repeat(60)] {
        check!(format!("add {} bytes", bad.len()), fs.tx(|fs| fs.add_entry(ROOT, bad, 7)), Err(FsError::NameTooLong));
    }
}

#[test]
fn a_directory_grows_past_one_block_and_past_twelve() {
    let g = geometry(512, 1000, 32);
    let mut fs = Fs::mkfs(MemDisk::new(512), &g).unwrap();
    fs.mkdir("/many").unwrap();
    for i in 0..800 {
        fs.create(&format!("/many/n{i:04}")).unwrap();
    }
    let s = fs.stat("/many").unwrap();
    // 802 entries of 64 bytes are 13 blocks: 13 data blocks and the indirect block
    check!("size and blocks", (s.size, s.blocks), (802 * 64, 14));
    let listed = names(&mut fs, "/many");
    check!("all listed", (listed.len(), listed[0].clone(), listed[799].clone()), (800, "n0000".into(), "n0799".into()));
}

#[test]
fn running_out_leaves_no_trace() {
    let mut fs = Fs::mkfs(MemDisk::new(256), &geometry(256, 4, 12)).unwrap();
    for name in ["/a", "/b", "/c"] {
        fs.create(name).unwrap();
    }
    let free = fs.free_blocks().unwrap();
    check!("no inode left", (fs.create("/d"), fs.mkdir("/d")), (Err(FsError::NoInodes), Err(FsError::NoInodes)));
    check!("nothing changed", (names(&mut fs, "/").len(), fs.free_blocks()), (3, Ok(free)));
    // a new directory needs a block: fill the disk first
    let mut fs = Fs::mkfs(MemDisk::new(28), &geometry(28, 8, 12)).unwrap();
    fs.create("/f").unwrap();
    let data = vec![1; fs.free_blocks().unwrap() as usize * BS];
    fs.write("/f", 0, &data).unwrap();
    let root = fs.stat("/").unwrap();
    check!("mkdir on a full disk", fs.mkdir("/d"), Err(FsError::NoSpace));
    check!("root unchanged", fs.stat("/"), Ok(root));
    check!("an empty file still fits", fs.create("/g").is_ok(), true);
}

#[test]
fn a_long_write_is_cut_into_whole_pieces() {
    for (journal, piece) in [(12u32, 2u64), (13, 3), (20, 10), (26, 16), (64, 16)] {
        let total = 200;
        let mut fs = Fs::mkfs(MemDisk::new(total), &geometry(total, 8, journal)).unwrap();
        fs.create("/f").unwrap();
        let offset = 3 * BS as u64 + 100;
        let data = vec![8; 400 * BS];
        check!(format!("journal {journal}: too much"), fs.write("/f", offset, &data), Err(FsError::NoSpace));
        let size = fs.stat("/f").unwrap().size;
        let blocks = size / BS as u64 - 3;
        check!(format!("journal {journal}: size {size} ends on a piece"), (size % BS as u64, blocks % piece, blocks > 0), (0, 0, true));
        let free = fs.free_blocks().unwrap();
        check!(format!("journal {journal}: stopped only when a piece did not fit"), free < piece + 1, true);
        check!(format!("journal {journal}: the kept part"), fs.read("/f", offset, 10), Ok(vec![8; 10]));
    }
}

#[test]
fn a_write_is_one_transaction_per_piece() {
    struct Ticks(std::rc::Rc<std::cell::Cell<u64>>);
    impl Clock for Ticks {
        fn now(&mut self) -> u64 {
            self.0.set(self.0.get() + 1);
            self.0.get()
        }
    }
    let disk = Fs::mkfs(MemDisk::new(256), &geometry(256, 8, 14)).unwrap().into_device();
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut fs = Fs::mount_with_clock(disk, Box::new(Ticks(count.clone()))).unwrap();
    fs.create("/f").unwrap();
    // pieces of 4 blocks: from byte 100 the first piece ends at 4 * 4096
    for (offset, len, pieces) in [(100, 4 * BS - 100, 1), (100, 4 * BS - 99, 2), (100, 12 * BS, 4), (0, 12 * BS, 3), (4095, 4 * BS, 2), (7, 1, 1), (7, 0, 0)] {
        let before = count.get();
        fs.write("/f", offset as u64, &vec![1; len]).unwrap();
        check!(format!("write {len} at {offset}"), count.get() - before, pieces);
    }
}

#[test]
fn damaged_directories_are_errors() {
    let cases: [(&str, [u8; 64]); 5] = [
        ("name length 0", {
            let mut e = raw_entry(3, b"q");
            e[4] = 0;
            e
        }),
        ("name length 60", {
            let mut e = raw_entry(3, b"q");
            e[4] = 60;
            e
        }),
        ("not UTF-8", raw_entry(3, &[0xFF, 0xFE])),
        ("a slash in a name", raw_entry(3, b"a/b")),
        ("a NUL in a name", raw_entry(3, b"a\0")),
    ];
    for (what, entry) in cases {
        let mut fs = new_fs();
        fs.mkdir("/d").unwrap();
        let d = fs.resolve("/d").unwrap();
        poke(&mut fs, d, 128, &entry);
        check!(format!("{what}: list"), matches!(fs.readdir("/d"), Err(FsError::Corrupt(_))), true);
        check!(format!("{what}: walk"), matches!(fs.resolve("/d/x"), Err(FsError::Corrupt(_))), true);
        check!(format!("{what}: create"), matches!(fs.create("/d/x"), Err(FsError::Corrupt(_))), true);
    }
}

#[test]
fn damaged_sizes_and_numbers_are_errors() {
    let mut fs = new_fs();
    let d = fs.mkdir("/d").unwrap();
    let sound = fs.load_inode(d).unwrap();
    for size in [100, 1u64 << 31, MAX_FILE_SIZE - 64, (256 - 36) * BS as u64 + 64] {
        fs.tx(|fs| fs.store_inode(d, &Inode { size, ..sound.clone() })).unwrap();
        check!(format!("directory of size {size}"), matches!(fs.readdir("/d"), Err(FsError::Corrupt(_))), true);
    }
    fs.tx(|fs| fs.store_inode(d, &sound)).unwrap();
    for ino in [65, 1000, u32::MAX] {
        fs.tx(|fs| fs.add_entry(d, "bad", ino)).unwrap();
        check!(format!("entry to inode {ino}"), matches!(fs.stat("/d/bad"), Err(FsError::Corrupt(_))), true);
        check!(format!("walk through inode {ino}"), matches!(fs.stat("/d/bad/x"), Err(FsError::Corrupt(_))), true);
        fs.tx(|fs| fs.remove_entry(d, "bad")).unwrap();
    }
}

#[test]
fn random_trees_match_a_map_of_paths() {
    use std::collections::BTreeMap;
    let mut rng = anneal_prelude::Rng::new(110_501);
    for round in 0..8 {
        let mut fs = Fs::mkfs(MemDisk::new(512), &geometry(512, 128, 20)).unwrap();
        // path -> None for a directory, or the file's bytes
        let mut model: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        for step in 0..150 {
            let pick = |rng: &mut anneal_prelude::Rng| ["a", "b", "c"][rng.below(3)];
            let path: String = (0..1 + rng.below(3)).map(|_| format!("/{}", pick(&mut rng))).collect();
            let parent = &path[..path.rfind('/').unwrap()];
            let parent_ok = parent.is_empty() || model.get(parent) == Some(&None);
            let what = format!("round {round} step {step}");
            match rng.below(4) {
                0 | 1 => {
                    let dir = rng.bool();
                    let free = parent_ok && !model.contains_key(&path);
                    let got = if dir { fs.mkdir(&path) } else { fs.create(&path) };
                    check!(format!("{what}: new {path} (dir: {dir})"), got.is_ok(), free);
                    if free {
                        model.insert(path.clone(), (!dir).then(Vec::new));
                    }
                }
                2 => {
                    let (offset, len) = (rng.below(30_000), rng.below(9000));
                    let byte = rng.below(256) as u8;
                    let got = fs.write(&path, offset as u64, &vec![byte; len]);
                    match model.get_mut(&path) {
                        Some(Some(data)) => {
                            check!(format!("{what}: write {path}"), got, Ok(()));
                            if len > 0 {
                                data.resize(data.len().max(offset + len), 0);
                                data[offset..offset + len].fill(byte);
                            }
                        }
                        _ => check!(format!("{what}: write {path} fails"), got.is_err(), true),
                    }
                }
                _ => {
                    let size = rng.below(30_000);
                    let got = fs.truncate(&path, size as u64);
                    match model.get_mut(&path) {
                        Some(Some(data)) => {
                            check!(format!("{what}: truncate {path}"), got, Ok(()));
                            data.resize(size, 0);
                        }
                        _ => check!(format!("{what}: truncate {path} fails"), got.is_err(), true),
                    }
                }
            }
        }
        let mut fs = Fs::mount(fs.into_device()).unwrap();
        for (path, content) in &model {
            let stat = fs.stat(path).unwrap();
            match content {
                None => {
                    let mut want: Vec<&str> = model
                        .keys()
                        .filter_map(|k| k.strip_prefix(path.as_str())?.strip_prefix('/'))
                        .filter(|rest| !rest.contains('/'))
                        .collect();
                    want.sort();
                    let subdirs = want.iter().filter(|n| model[&format!("{path}/{n}")].is_none()).count();
                    check!(format!("round {round}: {path}"), (stat.kind, names(&mut fs, path), usize::from(stat.nlink)), (Kind::Dir, want.into_iter().map(String::from).collect(), 2 + subdirs));
                }
                Some(data) => {
                    let got = fs.read(path, 0, 100_000).unwrap();
                    check!(format!("round {round}: {path}, {} bytes", data.len()), (stat.size, got == *data), (data.len() as u64, true));
                }
            }
        }
    }
}
