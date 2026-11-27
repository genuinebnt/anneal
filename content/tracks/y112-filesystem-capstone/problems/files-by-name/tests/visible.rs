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
fn the_root_is_there() {
    let mut fs = new_fs();
    let root = fs.stat("/").unwrap();
    check!("root: ino, kind, links, size, blocks", (root.ino, root.kind, root.nlink, root.size, root.blocks), (ROOT, Kind::Dir, 2, 128, 1));
    check!("empty", fs.readdir("/"), Ok(vec![]));
}

#[test]
fn make_a_tree_and_read_a_file() {
    let mut fs = new_fs();
    check!("mkdir", fs.mkdir("/usr").is_ok() && fs.mkdir("/usr/lib").is_ok(), true);
    check!("create", fs.create("/usr/lib/libc").is_ok(), true);
    check!("write", fs.write("/usr/lib/libc", 0, b"hello"), Ok(()));
    check!("read", fs.read("/usr/lib/libc", 0, 100), Ok(b"hello".to_vec()));
    check!("dots are real", fs.read("/usr/lib/../lib/./libc", 0, 100), Ok(b"hello".to_vec()));
    check!("names", names(&mut fs, "/usr"), vec!["lib".to_string()]);
}

#[test]
fn names_come_sorted() {
    let mut fs = new_fs();
    for path in ["/b", "/a", "/c", "/B"] {
        fs.create(path).unwrap();
    }
    check!("sorted by bytes", names(&mut fs, "/"), ["B", "a", "b", "c"].map(String::from).to_vec());
}

#[test]
fn a_directory_counts_its_subdirectories() {
    let mut fs = new_fs();
    fs.mkdir("/a").unwrap();
    fs.mkdir("/a/b").unwrap();
    fs.mkdir("/a/c").unwrap();
    fs.create("/a/f").unwrap();
    check!("links of /, /a, /a/b, /a/f", ["/", "/a", "/a/b", "/a/f"].map(|p| fs.stat(p).unwrap().nlink), [3, 4, 2, 1]);
}

#[test]
fn everything_is_there_after_a_remount() {
    let mut fs = new_fs();
    fs.mkdir("/d").unwrap();
    fs.create("/d/f").unwrap();
    fs.write("/d/f", 5000, b"later").unwrap();
    let mut fs = Fs::mount(fs.into_device()).unwrap();
    check!("size", fs.stat("/d/f").map(|s| s.size), Ok(5005));
    check!("bytes", fs.read("/d/f", 4998, 10), Ok(b"\0\0later".to_vec()));
}
