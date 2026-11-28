use solution::*;

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Write(u64),
    Sync,
}

/// A disk that writes down writes and syncs, and can be told to fail one write.
struct Spy {
    disk: MemDisk,
    calls: Vec<Call>,
    /// The write after this many more fails, once.
    fail_in: Option<u32>,
}

impl Spy {
    fn new(blocks: u64) -> Spy {
        Spy { disk: MemDisk::from_image(vec![0xEE; blocks as usize * BS]), calls: Vec::new(), fail_in: None }
    }
}

impl BlockDevice for Spy {
    fn num_blocks(&self) -> u64 {
        self.disk.num_blocks()
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        self.disk.read_block(n, buf)
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        if let Some(k) = self.fail_in {
            self.fail_in = k.checked_sub(1);
            if k == 0 {
                return Err(FsError::Io("injected"));
            }
        }
        self.calls.push(Call::Write(n));
        self.disk.write_block(n, buf)
    }
    fn sync(&mut self) -> R<()> {
        self.calls.push(Call::Sync);
        self.disk.sync()
    }
}

/// 256 blocks, 32 inodes and a journal of 16: data starts at block 21.
const SMALL: Geometry = Geometry { total_blocks: 256, inode_count: 32, journal_blocks: 16 };

fn new_fs() -> Fs<Spy> {
    Fs::format(Spy::new(256), &SMALL).unwrap()
}

fn image(fs: &mut Fs<Spy>) -> Vec<u8> {
    fs.device().disk.image().to_vec()
}

struct Countdown(u64);

impl Clock for Countdown {
    fn now(&mut self) -> u64 {
        self.0 -= 10;
        self.0
    }
}

#[test]
fn format_writes_the_superblock_last() {
    let mut spy = Spy::new(256);
    let old = Super::layout(&SMALL).unwrap().encode();
    spy.disk.write_block(0, &old).unwrap();
    let mut fs = Fs::format(spy, &SMALL).unwrap();
    let calls = fs.device().calls.clone();
    check!("first: block 0, then a sync", calls[..2].to_vec(), vec![Call::Write(0), Call::Sync]);
    check!("last: a sync, block 0, a sync", calls[calls.len() - 3..].to_vec(), vec![Call::Sync, Call::Write(0), Call::Sync]);
    check!("block 0 is written twice in all", calls.iter().filter(|c| **c == Call::Write(0)).count(), 2);
}

#[test]
fn a_format_that_stops_early_cannot_be_mounted() {
    let mut spy = Spy::new(256);
    spy.disk.write_block(0, &Super::layout(&SMALL).unwrap().encode()).unwrap();
    for k in 0..10 {
        spy.fail_in = Some(k);
        let failed = Fs::format(&mut spy, &SMALL).is_err();
        spy.fail_in = None;
        check!(format!("write {k} fails"), failed, true);
        if k >= 1 {
            check!(format!("mount after write {k} failed"), Fs::mount(&mut spy).err(), Some(FsError::BadMagic));
        }
    }
}

#[test]
fn format_starts_from_nothing() {
    let mut fs = new_fs();
    let sb = fs.superblock();
    let disk = image(&mut fs);
    check!("superblock", disk[..BS] == sb.encode()[..], true);
    let mut header = vec![0; BS];
    header[..4].copy_from_slice(&JMAGIC.to_le_bytes());
    check!("journal header: its magic and zeros", disk[BS..2 * BS] == header[..], true);
    check!("rest of the journal is zero", disk[2 * BS..sb.ibm_start as usize * BS].iter().all(|b| *b == 0), true);
    let ibm = &disk[sb.ibm_start as usize * BS..][..BS];
    check!("inode bitmap: only bit 0", (ibm[0], ibm[1..].iter().all(|b| *b == 0)), (1, true));
    let dbm = &disk[sb.dbm_start as usize * BS..][..BS];
    // bits 0 to 20 are metadata
    check!("data bitmap", (dbm[0], dbm[1], dbm[2], dbm[3..].iter().all(|b| *b == 0)), (0xFF, 0xFF, 0x1F, true));
    check!("inode table is zero", disk[sb.itable_start as usize * BS..sb.data_start as usize * BS].iter().all(|b| *b == 0), true);
}

#[test]
fn format_and_mount_refuse_what_does_not_fit() {
    check!("device smaller than the geometry", Fs::format(MemDisk::new(255), &SMALL).err(), Some(FsError::BadGeometry));
    let bad = Geometry { journal_blocks: 3, ..SMALL };
    check!("bad geometry", Fs::format(MemDisk::new(256), &bad).err(), Some(FsError::BadGeometry));
    check!("empty device", Fs::mount(MemDisk::new(0)).err(), Some(FsError::BadMagic));
    check!("blank device", Fs::mount(MemDisk::new(256)).err(), Some(FsError::BadMagic));
    let mut image = Fs::format(MemDisk::new(256), &SMALL).unwrap().into_device().image().to_vec();
    image.truncate(200 * BS);
    check!("cut image", Fs::mount(MemDisk::from_image(image)).err(), Some(FsError::BadGeometry));
}

#[test]
fn a_commit_writes_the_blocks_then_syncs() {
    let mut fs = new_fs();
    fs.device().calls.clear();
    let a = fs.tx(|fs| fs.alloc_block()).unwrap();
    let calls = fs.device().calls.clone();
    check!("ends with a sync", calls.last(), Some(&Call::Sync));
    check!("the new block was written", calls.contains(&Call::Write(u64::from(a))), true);
    let mut disk = image(&mut fs);
    check!("it is zeroed on the disk", disk[a as usize * BS..][..BS].iter().all(|b| *b == 0), true);
    disk.truncate(256 * BS);
    check!("remount sees it used", Fs::mount(MemDisk::from_image(disk)).unwrap().free_blocks(), Ok(256 - 22));
}

#[test]
fn changes_need_a_transaction_and_do_not_nest() {
    let mut fs = new_fs();
    check!("alloc_block", matches!(fs.alloc_block(), Err(FsError::Invalid(_))), true);
    check!("alloc_inode", matches!(fs.alloc_inode(K_FILE, 1), Err(FsError::Invalid(_))), true);
    check!("write_block", matches!(fs.write_block(30, &zero_block()), Err(FsError::Invalid(_))), true);
    check!("free blocks", fs.free_blocks(), Ok(235));
    let nested = fs.tx(|fs| {
        let inner: R<()> = fs.tx(|_| Ok(()));
        Ok(matches!(inner, Err(FsError::Invalid(_))))
    });
    check!("tx inside tx", nested, Ok(true));
    // the refused inner tx leaves the outer one open: it may still write
    let outer = fs.tx(|fs| {
        let _ = fs.tx(|_| Ok(()));
        fs.alloc_block()
    });
    check!("the outer tx goes on", outer.is_ok(), true);
    let after = fs.tx(|fs| fs.alloc_block());
    check!("the next transaction works", after.is_ok(), true);
}

#[test]
fn uncommitted_changes_are_seen_inside_only() {
    let mut fs = new_fs();
    let before = image(&mut fs);
    let seen = fs.tx(|fs| {
        let b = fs.alloc_block()?;
        let mut data = zero_block();
        data[0] = 5;
        fs.write_block(u64::from(b), &data)?;
        let seen = fs.read_block(u64::from(b))?[0];
        let untouched = fs.device().disk.image() == &before[..];
        Ok((seen, untouched))
    });
    check!("(read inside, disk untouched inside)", seen, Ok((5, true)));
}

#[test]
fn every_block_once_then_no_space() {
    let mut fs = new_fs();
    let mut got = Vec::new();
    let end = fs.tx(|fs| loop {
        match fs.alloc_block() {
            Ok(b) => got.push(b),
            Err(e) => return Ok(e),
        }
    });
    check!("last answer", end, Ok(FsError::NoSpace));
    check!("handed out", got, (21..256).collect::<Vec<u32>>());
    check!("free", fs.free_blocks(), Ok(0));
}

#[test]
fn the_search_goes_on_from_the_last_block_and_wraps() {
    let mut fs = new_fs();
    fs.tx(|fs| {
        for _ in 0..10 {
            fs.alloc_block()?;
        }
        fs.free_block(23)?;
        fs.free_block(22)
    })
    .unwrap();
    let next = fs.tx(|fs| fs.alloc_block());
    check!("after 30 comes 31, not the freed 22", next, Ok(31));
    let rest: R<Vec<u32>> = fs.tx(|fs| (32..256).map(|_| fs.alloc_block()).collect());
    check!("up to the end", rest.map(|r| r.last().copied()), Ok(Some(255)));
    let wrapped = fs.tx(|fs| Ok([fs.alloc_block()?, fs.alloc_block()?]));
    check!("then around", wrapped, Ok([22, 23]));
}

#[test]
fn the_search_starts_at_the_data_area_after_a_mount() {
    let mut fs = new_fs();
    fs.tx(|fs| {
        fs.alloc_block()?;
        fs.alloc_block()?;
        fs.free_block(21)
    })
    .unwrap();
    let mut fs = Fs::mount(fs.into_device()).unwrap();
    check!("first free block", fs.tx(|fs| fs.alloc_block()), Ok(21));
}

#[test]
fn a_reused_block_is_zeroed() {
    let mut fs = new_fs();
    fs.tx(|fs| {
        let b = fs.alloc_block()?;
        let mut data = zero_block();
        data.fill(0xAB);
        fs.write_block(u64::from(b), &data)
    })
    .unwrap();
    fs.tx(|fs| fs.free_block(21)).unwrap();
    let mut fs = Fs::mount(fs.into_device()).unwrap();
    let zero = fs.tx(|fs| {
        let b = fs.alloc_block()?;
        Ok((b, fs.read_block(u64::from(b))?.iter().all(|x| *x == 0)))
    });
    check!("block 21 again", zero, Ok((21, true)));
}

#[test]
fn freeing_what_is_not_yours_is_corruption() {
    let mut fs = new_fs();
    fs.tx(|fs| fs.alloc_block()).unwrap();
    for n in [0, 1, 20, 256, u32::MAX, 22] {
        let got = fs.tx(|fs| fs.free_block(n));
        check!(format!("free_block({n})"), matches!(got, Err(FsError::Corrupt(_))), true);
    }
    check!("21 once", fs.tx(|fs| fs.free_block(21)), Ok(()));
    check!("21 twice", matches!(fs.tx(|fs| fs.free_block(21)), Err(FsError::Corrupt(_))), true);
    check!("free", fs.free_blocks(), Ok(235));
}

#[test]
fn a_bitmap_that_offers_the_superblock_is_not_believed() {
    let mut fs = new_fs();
    let dbm = fs.superblock().dbm_start;
    // mark every data block used and the superblock free
    fs.tx(|fs| {
        for b in 21..256 {
            fs.set_bit(dbm, b, true)?;
        }
        fs.set_bit(dbm, 0, false)
    })
    .unwrap();
    let before = image(&mut fs);
    check!("alloc", matches!(fs.tx(|fs| fs.alloc_block()), Err(FsError::Corrupt(_))), true);
    check!("the disk is unchanged", image(&mut fs) == before, true);
}

#[test]
fn block_numbers_out_of_range() {
    let mut fs = new_fs();
    for n in [256, 1 << 40, u64::MAX] {
        check!(format!("read {n}"), matches!(fs.read_block(n), Err(FsError::Corrupt(_))), true);
        let w = fs.tx(|fs| fs.write_block(n, &zero_block()));
        check!(format!("write {n}"), matches!(w, Err(FsError::Corrupt(_))), true);
    }
}

#[test]
fn inode_records_are_where_the_layout_says() {
    let mut fs = new_fs();
    let record = Inode {
        kind: K_FILE,
        nlink: 0x0102,
        mode: 0o600,
        size: 0x1122_3344,
        mtime: 7,
        ctime: 8,
        direct: [30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41],
        indirect: 42,
        double: 43,
    };
    let mut want = [0u8; 128];
    want[0] = 1;
    want[2..4].copy_from_slice(&0x0102u16.to_le_bytes());
    want[4..6].copy_from_slice(&0o600u16.to_le_bytes());
    want[8..16].copy_from_slice(&0x1122_3344u64.to_le_bytes());
    want[16..24].copy_from_slice(&7u64.to_le_bytes());
    want[24..32].copy_from_slice(&8u64.to_le_bytes());
    for i in 0..14u32 {
        want[32 + 4 * i as usize..36 + 4 * i as usize].copy_from_slice(&(30 + i).to_le_bytes());
    }
    let mut out = [0xFFu8; 128];
    record.encode(&mut out);
    check!("encode", out.to_vec(), want.to_vec());
    check!("decode", Inode::decode(&want), record.clone());
    fs.tx(|fs| fs.store_inode(32, &record)).unwrap();
    fs.tx(|fs| fs.store_inode(31, &Inode { kind: K_DIR, ..Inode::default() })).unwrap();
    let sb = fs.superblock();
    let disk = image(&mut fs);
    // inode 32 is the first record of the second table block
    check!("record 32 on the disk", disk[(sb.itable_start as usize + 1) * BS..][..128].to_vec(), want.to_vec());
    check!("record 31 beside it", disk[sb.itable_start as usize * BS + 31 * 128], K_DIR);
    check!("load", fs.load_inode(32), Ok(record));
}

#[test]
fn bad_inode_numbers_and_records() {
    let mut fs = new_fs();
    for n in [0, 33, u32::MAX] {
        check!(format!("load_inode({n})"), matches!(fs.load_inode(n), Err(FsError::Corrupt(_))), true);
        let s = fs.tx(|fs| fs.store_inode(n, &Inode::default()));
        check!(format!("store_inode({n})"), matches!(s, Err(FsError::Corrupt(_))), true);
    }
    for bad in [Inode { kind: 3, ..Inode::default() }, Inode { kind: K_FILE, size: MAX_FILE_SIZE + 1, ..Inode::default() }] {
        fs.tx(|fs| fs.store_inode(4, &bad)).unwrap();
        check!(format!("kind {} size {}", bad.kind, bad.size), matches!(fs.load_inode(4), Err(FsError::Corrupt(_))), true);
    }
    let largest = Inode { kind: K_FILE, size: MAX_FILE_SIZE, ..Inode::default() };
    fs.tx(|fs| fs.store_inode(4, &largest)).unwrap();
    check!("the largest size is fine", fs.load_inode(4), Ok(largest));
}

#[test]
fn inodes_run_out_and_come_back() {
    let mut fs = new_fs();
    let mut got = Vec::new();
    let end = fs.tx(|fs| loop {
        match fs.alloc_inode(K_FILE, 1) {
            Ok(i) => got.push(i),
            Err(e) => return Ok(e),
        }
    });
    check!("32 inodes then", (got, end), ((1..=32).collect::<Vec<u32>>(), Ok(FsError::NoInodes)));
    check!("free 7", fs.tx(|fs| fs.free_inode(7)), Ok(()));
    check!("record 7 is blank", fs.load_inode(7), Ok(Inode::default()));
    check!("7 again", fs.tx(|fs| fs.alloc_inode(K_DIR, 2)), Ok(7));
}

#[test]
fn the_inode_search_goes_on_from_the_last_number() {
    let mut fs = new_fs();
    let got = fs.tx(|fs| Ok([fs.alloc_inode(K_FILE, 1)?, fs.alloc_inode(K_FILE, 1)?, fs.alloc_inode(K_FILE, 1)?]));
    check!("1, 2, 3", got, Ok([1, 2, 3]));
    fs.tx(|fs| fs.free_inode(1)).unwrap();
    check!("after 3 comes 4, not the freed 1", fs.tx(|fs| fs.alloc_inode(K_FILE, 1)), Ok(4));
}

#[test]
fn bits_past_the_last_block_are_not_counted() {
    let mut fs = new_fs();
    let dbm = fs.superblock().dbm_start;
    // a damaged bitmap may set bits that stand for no block
    fs.tx(|fs| (256..300).try_for_each(|b| fs.set_bit(dbm, b, true))).unwrap();
    check!("free blocks", fs.free_blocks(), Ok(235));
}

#[test]
fn times_and_links_come_from_the_transaction() {
    let disk = Fs::format(MemDisk::new(256), &SMALL).unwrap().into_device();
    let mut fs = Fs::mount_with_clock(disk, Box::new(Countdown(1000))).unwrap();
    let ino = fs.tx(|fs| fs.alloc_inode(K_DIR, 2)).unwrap();
    let new = fs.load_inode(ino).unwrap();
    check!("one tick for the transaction", (new.mode, new.mtime, new.ctime), (0o755, 990, 990));
    fs.tx(|fs| {
        fs.adjust_links(ino, 3)?;
        fs.adjust_links(ino, -1)
    })
    .unwrap();
    let after = fs.load_inode(ino).unwrap();
    check!("links, mtime, ctime", (after.nlink, after.mtime, after.ctime), (4, 990, 980));
    for change in [-5, 70_000] {
        let got = fs.tx(|fs| fs.adjust_links(ino, change));
        check!(format!("change {change}"), matches!(got, Err(FsError::Invalid(_))), true);
    }
}

#[test]
fn a_commit_that_fails_stops_everything() {
    for k in 0..4 {
        let mut fs = new_fs();
        let got = fs.tx(|fs| {
            fs.alloc_inode(K_FILE, 1)?;
            fs.alloc_block()?;
            fs.device().fail_in = Some(k);
            Ok(())
        });
        check!(format!("write {k}: the commit"), got, Err(FsError::Io("injected")));
        check!(format!("write {k}: next tx"), fs.tx(|_| Ok(1)), Err(FsError::Poisoned));
        check!(format!("write {k}: free_blocks"), fs.free_blocks(), Err(FsError::Poisoned));
        check!(format!("write {k}: load_inode"), fs.load_inode(1), Err(FsError::Poisoned));
        check!(format!("write {k}: a new mount works"), Fs::mount(fs.into_device()).is_ok(), true);
    }
}

#[test]
fn a_large_disk_has_two_bitmap_blocks() {
    let g = Geometry { total_blocks: 33_000, inode_count: 8, journal_blocks: 12 };
    let mut fs = Fs::format(MemDisk::new(33_000), &g).unwrap();
    let start = fs.superblock().data_start;
    check!("free", fs.free_blocks(), Ok(33_000 - start));
    let dbm = fs.superblock().dbm_start;
    fs.tx(|fs| (start..32_999).try_for_each(|b| if b == 32_800 { Ok(()) } else { fs.set_bit(dbm, b, true) })).unwrap();
    let got = fs.tx(|fs| Ok([fs.alloc_block()?, fs.alloc_block()?]));
    check!("the two that are left, in the second bitmap block", got, Ok([32_800, 32_999]));
}

#[test]
fn random_use_matches_a_set() {
    let mut rng = anneal_prelude::Rng::new(110_301);
    let mut fs = new_fs();
    let mut used: std::collections::BTreeSet<u32> = Default::default();
    let mut next = 21u32;
    for step in 0..400 {
        if rng.below(5) < 3 || used.is_empty() {
            // the first free block at or after `next`, going round
            let want = (0..235).map(|i| 21 + (next - 21 + i) % 235).find(|b| !used.contains(b));
            let got = fs.tx(|fs| fs.alloc_block());
            check!(format!("step {step}: alloc"), got.clone(), want.ok_or(FsError::NoSpace));
            if let Ok(b) = got {
                used.insert(b);
                next = if b == 255 { 21 } else { b + 1 };
            }
        } else {
            let b = *used.iter().nth(rng.below(used.len())).unwrap();
            used.remove(&b);
            check!(format!("step {step}: free {b}"), fs.tx(|fs| fs.free_block(b)), Ok(()));
        }
        check!(format!("step {step}: free count"), fs.free_blocks(), Ok(235 - used.len() as u64));
    }
}
