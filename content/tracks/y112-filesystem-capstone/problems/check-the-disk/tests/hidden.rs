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
fn an_orphan_goes_with_its_blocks() {
    let mut fs = tree();
    let free = fs.free_blocks().unwrap();
    let block = first_block(&mut fs, 6);
    fs.tx(|fs| fs.remove_entry(ROOT, "top")).unwrap();
    check!("found", problems(&mut fs), vec![OrphanInode { ino: 6 }, LeakedBlock { block }]);
    check!("repaired", fs.fsck(true).map(|r| r.repaired), Ok(2));
    check!("now clean", problems(&mut fs), vec![]);
    check!("block and inode are free", (fs.free_blocks(), fs.load_inode(6)), (Ok(free + 1), Ok(Inode::default())));
}

#[test]
fn a_lost_inode_bit_must_not_lose_the_file() {
    for ino in [3, 5, 2] {
        let mut fs = tree();
        let before = files(&mut fs);
        let free = fs.free_blocks().unwrap();
        inode_bit(&mut fs, ino, false);
        check!(format!("inode {ino}: found"), problems(&mut fs), vec![UnmarkedInode { ino }]);
        check!(format!("inode {ino}: repaired"), fs.fsck(true).map(|r| r.repaired), Ok(1));
        check!(format!("inode {ino}: clean"), problems(&mut fs), vec![]);
        check!(format!("inode {ino}: files and space"), (files(&mut fs) == before, fs.free_blocks()), (true, Ok(free)));
    }
}

#[test]
fn entries_that_lead_nowhere() {
    for (target, name) in [(20, "free"), (0x7FFF_FFFF, "huge"), (33, "past")] {
        let mut fs = tree();
        fs.tx(|fs| fs.add_entry(ROOT, name, target)).unwrap();
        block_bit(&mut fs, 95, true);
        block_bit(&mut fs, 94, false);
        let want = vec![BadEntry { dir: 1, name: name.to_string() }, LeakedBlock { block: 95 }];
        check!(format!("entry to {target}"), problems(&mut fs), want.clone());
        // the walk was not complete, so the leak stays
        check!("repair", fs.fsck(true).map(|r| r.repaired), Ok(0));
        check!("still there", problems(&mut fs), want);
    }
}

#[test]
fn damaged_inodes_are_not_freed() {
    for (what, change) in [("kind 9", (|i: &mut Inode| i.kind = 9) as fn(&mut Inode)), ("size", |i| i.size = u64::MAX)] {
        let mut fs = tree();
        edit_inode(&mut fs, 4, change);
        block_bit(&mut fs, 95, true);
        let report = fs.fsck(true).unwrap();
        check!(format!("{what}: bad entry"), report.problems.contains(&BadEntry { dir: 2, name: "g".into() }), true);
        check!(format!("{what}: orphan reported"), report.problems.contains(&OrphanInode { ino: 4 }), true);
        check!(format!("{what}: nothing repaired"), report.repaired, 0);
        let still_marked = fs.bit(fs.superblock().ibm_start, 4);
        check!(format!("{what}: record kept"), still_marked, Ok(true));
    }
}

#[test]
fn a_bad_block_tree_stops_every_repair_that_frees() {
    let mut fs = tree();
    edit_inode(&mut fs, 3, |i| i.indirect = 2);
    edit_inode(&mut fs, 6, |i| i.nlink = 9);
    block_bit(&mut fs, 95, true);
    inode_bit(&mut fs, 4, false);
    let used = first_block(&mut fs, 4);
    block_bit(&mut fs, used, false);
    let report = fs.fsck(true).unwrap();
    check!("bad tree", report.problems.contains(&BadBlockTree { ino: 3 }), true);
    check!("only the two marks are repaired", report.repaired, 2);
    let after = problems(&mut fs);
    check!("link count and leak stay", after.contains(&LinkCount { ino: 6, stored: 9, found: 1 }) && after.contains(&LeakedBlock { block: 95 }), true);
    check!("marks are back", after.iter().any(|p| matches!(p, UnmarkedInode { .. } | UnmarkedBlock { .. })), false);
}

#[test]
fn a_directory_with_a_bad_tree_or_a_bad_size() {
    for (what, change) in [
        ("pointer into the journal", (|i: &mut Inode| i.direct[0] = 3) as fn(&mut Inode)),
        ("odd size", |i| i.size = 100),
        ("2 GiB", |i| i.size = 1 << 31),
    ] {
        let mut fs = tree();
        edit_inode(&mut fs, 2, change);
        let free = fs.free_blocks().unwrap();
        let report = fs.fsck(true).unwrap();
        check!(what, report.problems.contains(&BadDirectory { ino: 2 }), true);
        check!(format!("{what}: nothing freed"), (report.repaired, fs.free_blocks()), (0, Ok(free)));
    }
}

#[test]
fn a_root_that_is_not_a_directory() {
    for kind in [K_FREE, K_FILE, 77] {
        let mut fs = tree();
        edit_inode(&mut fs, 1, |i| i.kind = kind);
        let report = fs.fsck(true).unwrap();
        check!(format!("kind {kind}"), report.problems.contains(&BadDirectory { ino: 1 }), true);
        check!(format!("kind {kind}: nothing repaired"), report.repaired, 0);
    }
}

#[test]
fn cross_linked_blocks_are_reported_and_left() {
    let mut fs = tree();
    let shared = first_block(&mut fs, 3);
    let lost = first_block(&mut fs, 4);
    edit_inode(&mut fs, 4, |i| i.direct[0] = shared as u32);
    check!("found", problems(&mut fs), vec![CrossLink { block: shared }, LeakedBlock { block: lost }]);
    // the walk was complete, so the leak can go; the shared block cannot be fixed here
    check!("repaired", fs.fsck(true).map(|r| r.repaired), Ok(1));
    check!("left", problems(&mut fs), vec![CrossLink { block: shared }]);
    let mut fs = tree();
    let mine = first_block(&mut fs, 4);
    edit_inode(&mut fs, 4, |i| i.direct[1] = mine as u32);
    check!("twice in one file", problems(&mut fs), vec![CrossLink { block: mine }]);
}

#[test]
fn dots_and_duplicate_names() {
    let mut fs = tree();
    fs.tx(|fs| fs.set_entry(5, "..", 1)).unwrap();
    let got = problems(&mut fs);
    let want = vec![
        WrongDots { dir: 5, name: "..".into() },
        LinkCount { ino: 1, stored: 3, found: 4 },
        LinkCount { ino: 2, stored: 3, found: 2 },
    ];
    check!("'..' to the root", got, want);

    let mut fs = tree();
    fs.tx(|fs| fs.remove_entry(5, "..")).unwrap();
    check!("no '..'", problems(&mut fs), vec![MissingDots { dir: 5 }, LinkCount { ino: 2, stored: 3, found: 2 }]);

    let mut fs = tree();
    fs.tx(|fs| fs.set_entry(5, ".", 2)).unwrap();
    check!("'.' elsewhere", problems(&mut fs).contains(&WrongDots { dir: 5, name: ".".into() }), true);

    let mut fs = tree();
    fs.tx(|fs| {
        fs.adjust_links(3, 1)?;
        fs.add_entry(2, "f", 3)
    })
    .unwrap();
    check!("the same name twice", problems(&mut fs), vec![DuplicateName { dir: 2, name: "f".into() }]);
}

#[test]
fn naming_damage_does_not_stop_repairs() {
    // only BadDirectory, BadEntry and BadBlockTree make the walk incomplete
    let mut fs = tree();
    fs.tx(|fs| fs.set_entry(5, "..", 1)).unwrap();
    block_bit(&mut fs, 95, true);
    check!("wrong '..': two link counts and a leak", fs.fsck(true).map(|r| r.repaired), Ok(3));
    check!("wrong '..': what is left", problems(&mut fs), vec![WrongDots { dir: 5, name: "..".into() }]);

    let mut fs = tree();
    fs.tx(|fs| fs.add_entry(2, "f", 3)).unwrap();
    block_bit(&mut fs, 95, true);
    check!("a name twice: a link count and a leak", fs.fsck(true).map(|r| r.repaired), Ok(2));
    check!("a name twice: what is left", problems(&mut fs), vec![DuplicateName { dir: 2, name: "f".into() }]);

    let mut fs = tree();
    fs.tx(|fs| fs.remove_entry(5, ".")).unwrap();
    block_bit(&mut fs, 95, true);
    check!("no '.': a link count and a leak", fs.fsck(true).map(|r| r.repaired), Ok(2));
    check!("no '.': what is left", problems(&mut fs), vec![MissingDots { dir: 5 }]);
}

#[test]
fn directory_cycles_end() {
    let mut fs = tree();
    // /d/sub/again is /d: a loop. And the root appears inside sub as well.
    fs.tx(|fs| {
        fs.add_entry(5, "again", 2)?;
        fs.add_entry(5, "top-again", 1)
    })
    .unwrap();
    let got = problems(&mut fs);
    check!("both are named twice", got.contains(&DirectoryTwice { ino: 1 }) && got.contains(&DirectoryTwice { ino: 2 }), true);
    check!("repair ends too", fs.fsck(true).is_ok(), true);
    check!("a second check ends", problems(&mut fs).contains(&DirectoryTwice { ino: 2 }), true);
}

#[test]
fn many_fixes_fit_the_smallest_journal() {
    let g = Geometry { total_blocks: 600, inode_count: 520, journal_blocks: 12 };
    let mut fs = Fs::mkfs(MemDisk::new(600), &g).unwrap();
    for i in 0..500 {
        fs.create(&format!("/{i}")).unwrap();
    }
    // 16 inode table blocks need a fix each: more than one transaction can hold
    for ino in (2..=500).step_by(32) {
        edit_inode(&mut fs, ino, |i| i.nlink = 3);
    }
    check!("repaired", fs.fsck(true).map(|r| r.repaired), Ok(16));
    check!("clean", problems(&mut fs), vec![]);
}

#[test]
fn many_orphans_fit_the_smallest_journal() {
    let g = Geometry { total_blocks: 600, inode_count: 520, journal_blocks: 12 };
    let mut fs = Fs::mkfs(MemDisk::new(600), &g).unwrap();
    // sixteen records that nothing names, in sixteen different table blocks:
    // freeing one changes its table block and the inode bitmap
    let ibm = fs.superblock().ibm_start;
    for ino in (2..=500u32).step_by(32) {
        fs.tx(|fs| {
            fs.store_inode(ino, &Inode { kind: K_FILE, nlink: 1, ..Inode::default() })?;
            fs.set_bit(ibm, u64::from(ino), true)
        })
        .unwrap();
    }
    check!("repaired", fs.fsck(true).map(|r| r.repaired), Ok(16));
    check!("clean", problems(&mut fs), vec![]);
}

#[test]
fn repair_is_made_of_transactions() {
    let mut fs = tree();
    block_bit(&mut fs, 95, true);
    edit_inode(&mut fs, 3, |i| i.nlink = 9);
    let planted = problems(&mut fs);
    let image = fs.into_device().image().to_vec();
    for k in 0..20 {
        let mut fs = Fs::mount(FaultDisk::from_image(image.clone())).unwrap();
        fs.device().arm(Some(k));
        let done = fs.fsck(true).is_ok();
        for seed in 1..10 {
            let cut = fs.device().post_crash_image(CrashMode::Chaos(seed));
            let mut back = Fs::mount(MemDisk::from_image(cut)).unwrap();
            // however the fixes are grouped, each one is there or not: a cut never adds a new kind of damage
            let left = problems(&mut back);
            let only_planted = left.iter().all(|p| planted.contains(p));
            check!(format!("cut after {k} writes, seed {seed}: {left:?}"), only_planted && (!done || left.is_empty()), true);
        }
    }
}

#[test]
fn bitmap_and_count_damage_is_always_repaired() {
    let mut rng = anneal_prelude::Rng::new(110_901);
    for round in 0..80 {
        let mut fs = tree();
        let before = files(&mut fs);
        let free = fs.free_blocks().unwrap();
        for _ in 0..1 + rng.below(8) {
            match rng.below(3) {
                0 => {
                    let b = 20 + rng.below(76) as u64;
                    let now = fs.bit(fs.superblock().dbm_start, b).unwrap();
                    block_bit(&mut fs, b, !now);
                }
                1 => {
                    // not the root: a mount would still work, but keep the record kinds
                    let ino = 2 + rng.below(5) as u32;
                    inode_bit(&mut fs, ino, rng.bool());
                }
                _ => {
                    let ino = 1 + rng.below(6) as u32;
                    let n = rng.below(5) as u16;
                    edit_inode(&mut fs, ino, |i| i.nlink = n);
                }
            }
        }
        let found = problems(&mut fs).len();
        check!(format!("round {round}: repaired all"), fs.fsck(true).map(|r| r.repaired), Ok(found));
        check!(format!("round {round}: clean"), problems(&mut fs), vec![]);
        check!(format!("round {round}: files and space"), (files(&mut fs) == before, fs.free_blocks()), (true, Ok(free)));
    }
}

#[test]
fn random_damage_never_panics_and_repair_loses_nothing() {
    let mut rng = anneal_prelude::Rng::new(110_902);
    let good = tree().into_device().image().to_vec();
    let data_start = Super::layout(&G).unwrap().data_start as usize;
    let mut mounted = 0;
    for round in 0..250 {
        let mut image = good.clone();
        for _ in 0..1 + rng.below(8) {
            // mostly the maps, the inode table and the first data blocks, where the directories are
            let block = if rng.below(10) == 0 { 1 + rng.below(95) } else { 17 + rng.below(data_start - 17 + 6) };
            let at = block * BS + if rng.below(4) == 0 { rng.below(BS) } else { rng.below(900) };
            if rng.bool() {
                image[at] = rng.below(256) as u8;
            } else {
                // a small number, as a plausible block or inode number
                let end = (at + 4).min(image.len());
                let value = (rng.below(100) as u32).to_le_bytes();
                image[at..end].copy_from_slice(&value[..end - at]);
            }
        }
        let Ok(mut fs) = Fs::mount(MemDisk::from_image(image)) else { continue };
        mounted += 1;
        let before = files(&mut fs);
        let _ = fs.fsck(false);
        let _ = fs.fsck(true);
        let after = files(&mut fs);
        for (path, content) in &before {
            check!(format!("round {round}: {path} reads as it did before the repair"), after.get(path) == Some(content), true);
        }
        let _ = fs.fsck(false);
        // every public call must come back, with a value or an error
        let _ = fs.free_blocks();
        let _ = fs.stat("/d/f");
        let _ = fs.write("/d/f", 100, b"hi");
        let _ = fs.write("/d/new", 0, b"x");
        let _ = fs.create("/d/new");
        let _ = fs.mkdir("/d/dir");
        let _ = fs.link("/d/f", "/linked");
        let _ = fs.rename("/d/sub", "/moved");
        let _ = fs.rename("/d", "/d/sub/inside");
        let _ = fs.truncate("/d/f", 5);
        let _ = fs.unlink("/d/g");
        let _ = fs.rmdir("/moved");
        let _ = fs.unlink("/top");
    }
    check!("most damaged disks still mount", mounted > 200, true);
}
