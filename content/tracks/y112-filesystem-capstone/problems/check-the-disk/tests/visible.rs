use solution::Problem::*;
use solution::*;
use std::collections::{BTreeMap, HashSet};

const G: Geometry = Geometry { total_blocks: 96, inode_count: 32, journal_blocks: 16 };

/// /d with the files /d/f (3 blocks) and /d/g (1 block), the empty directory /d/sub and the file /top (1 block).
/// The inode numbers: / 1, /d 2, /d/f 3, /d/g 4, /d/sub 5, /top 6.
fn tree() -> Fs<MemDisk> {
    let mut fs = Fs::mkfs(MemDisk::new(96), &G).unwrap();
    fs.mkdir("/d").unwrap();
    fs.create("/d/f").unwrap();
    fs.write("/d/f", 0, &[b'f'; 3 * BS]).unwrap();
    fs.create("/d/g").unwrap();
    fs.write("/d/g", 0, &[b'g'; 100]).unwrap();
    fs.mkdir("/d/sub").unwrap();
    fs.create("/top").unwrap();
    fs.write("/top", 0, b"top").unwrap();
    fs
}

fn edit_inode(fs: &mut Fs<MemDisk>, ino: u32, change: impl FnOnce(&mut Inode)) {
    let mut inode = Inode::default();
    fs.tx(|fs| {
        let (table, n) = (fs.superblock().itable_start, u64::from(ino));
        let raw = fs.read_block(table + n / 32)?;
        inode = Inode::decode(&raw[(n % 32) as usize * 128..][..128]);
        change(&mut inode);
        fs.store_inode(ino, &inode)
    })
    .unwrap();
}

fn inode_bit(fs: &mut Fs<MemDisk>, ino: u32, value: bool) {
    let start = fs.superblock().ibm_start;
    fs.tx(|fs| fs.set_bit(start, u64::from(ino), value)).unwrap();
}

fn block_bit(fs: &mut Fs<MemDisk>, block: u64, value: bool) {
    let start = fs.superblock().dbm_start;
    fs.tx(|fs| fs.set_bit(start, block, value)).unwrap();
}

fn first_block(fs: &mut Fs<MemDisk>, ino: u32) -> u64 {
    u64::from(fs.load_inode(ino).unwrap().direct[0])
}

fn problems(fs: &mut Fs<MemDisk>) -> Vec<Problem> {
    fs.fsck(false).unwrap().problems
}

/// Every file that can be read by walking names: path to its first 64 KiB and its size. Damage ends a branch.
fn files(fs: &mut Fs<MemDisk>) -> BTreeMap<String, (u64, Vec<u8>)> {
    let mut found = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut queue = vec![String::new()];
    while let Some(dir) = queue.pop() {
        let Ok(names) = fs.readdir(&format!("{dir}/")) else { continue };
        for (name, ino) in names {
            let path = format!("{dir}/{name}");
            match fs.stat(&path) {
                Ok(s) if s.kind == Kind::Dir => {
                    if seen.insert(ino) && path.len() < 200 {
                        queue.push(path);
                    }
                }
                Ok(s) => {
                    if let Ok(bytes) = fs.read(&path, 0, 1 << 16) {
                        found.insert(path, (s.size, bytes));
                    }
                }
                Err(_) => {}
            }
        }
    }
    found
}

#[test]
fn a_healthy_tree_has_no_problems() {
    let mut fs = tree();
    let report = fs.fsck(false).unwrap();
    check!("problems", report.problems, vec![]);
    // 6 inodes; blocks: 1 for each of the 3 directories, 3 + 1 + 1 for the files
    check!("inodes and blocks", (report.inodes, report.blocks), (6, 8));
}

#[test]
fn a_wrong_link_count_is_found_and_fixed() {
    let mut fs = tree();
    edit_inode(&mut fs, 3, |i| i.nlink = 7);
    check!("found", problems(&mut fs), vec![LinkCount { ino: 3, stored: 7, found: 1 }]);
    check!("repaired", fs.fsck(true).map(|r| r.repaired), Ok(1));
    check!("now clean", problems(&mut fs), vec![]);
}

#[test]
fn a_leaked_block_and_a_block_marked_free() {
    let mut fs = tree();
    let used = first_block(&mut fs, 4);
    block_bit(&mut fs, used, false);
    block_bit(&mut fs, 95, true);
    check!("found", problems(&mut fs), vec![UnmarkedBlock { block: used }, LeakedBlock { block: 95 }]);
    check!("repaired", fs.fsck(true).map(|r| r.repaired), Ok(2));
    check!("now clean", problems(&mut fs), vec![]);
}

#[test]
fn checking_alone_changes_nothing() {
    let mut fs = tree();
    block_bit(&mut fs, 95, true);
    let before = fs.device().image().to_vec();
    let report = fs.fsck(false).unwrap();
    check!("repaired nothing", report.repaired, 0);
    check!("disk unchanged", fs.device().image() == &before[..], true);
}

#[test]
fn a_directory_that_cannot_be_read_costs_nothing() {
    let mut fs = tree();
    let block = first_block(&mut fs, 2);
    fs.tx(|fs| {
        let mut b = fs.read_block(block)?;
        b[4] = 200; // the name length of the first entry
        fs.write_block(block, &b)
    })
    .unwrap();
    let free = fs.free_blocks().unwrap();
    let report = fs.fsck(true).unwrap();
    check!("the bad directory", report.problems.contains(&BadDirectory { ino: 2 }), true);
    check!("its files are orphans for now", report.problems.contains(&OrphanInode { ino: 3 }), true);
    check!("but nothing is freed", (report.repaired, fs.free_blocks()), (0, Ok(free)));
    check!("the hidden file is intact", fs.load_inode(3).map(|i| i.size), Ok(3 * BS as u64));
}
