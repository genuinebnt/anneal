use solution::*;
use std::collections::BTreeMap;

/// 64 blocks, 16 inodes, a journal of 16: data starts at block 20.
const G: Geometry = Geometry { total_blocks: 64, inode_count: 16, journal_blocks: 16 };

fn filled(byte: u8) -> Box<Block> {
    let mut b = zero_block();
    b.fill(byte);
    b
}

/// Everything a user can see: each path with its kind, links and bytes, and the number of free blocks.
fn snapshot<D: BlockDevice>(fs: &mut Fs<D>) -> (BTreeMap<String, (Kind, u16, Vec<u8>)>, u64) {
    let mut tree = BTreeMap::new();
    let mut queue = vec!["/".to_string()];
    while let Some(path) = queue.pop() {
        let stat = fs.stat(&path).unwrap();
        let bytes = match stat.kind {
            Kind::Dir => {
                for (name, _) in fs.readdir(&path).unwrap() {
                    queue.push(format!("{}/{name}", path.trim_end_matches('/')));
                }
                Vec::new()
            }
            Kind::File => fs.read(&path, 0, 1 << 20).unwrap(),
        };
        assert!(tree.len() < 100, "the tree does not end");
        tree.insert(path, (stat.kind, stat.nlink, bytes));
    }
    (tree, fs.free_blocks().unwrap())
}

fn base() -> Vec<u8> {
    let mut fs = Fs::mkfs(MemDisk::new(64), &G).unwrap();
    fs.mkdir("/d").unwrap();
    fs.create("/d/f").unwrap();
    fs.write("/d/f", 0, &[b'f'; 6000]).unwrap();
    fs.create("/g").unwrap();
    fs.mkdir("/empty").unwrap();
    fs.into_device().image().to_vec()
}

/// Puts a transaction into the journal of an image by hand: header, copies and, if `commit`, the commit record.
fn journal(image: &mut [u8], sequence: u64, blocks: &[(u64, u8)], commit: bool) {
    let mut header = zero_block();
    header[..4].copy_from_slice(&JMAGIC.to_le_bytes());
    header[8..16].copy_from_slice(&sequence.to_le_bytes());
    header[16..20].copy_from_slice(&(blocks.len() as u32).to_le_bytes());
    for (i, (target, _)) in blocks.iter().enumerate() {
        header[32 + 4 * i..36 + 4 * i].copy_from_slice(&(*target as u32).to_le_bytes());
    }
    let mut sum = crc32(&header[..]);
    image[BS..2 * BS].copy_from_slice(&header[..]);
    for (i, (_, byte)) in blocks.iter().enumerate() {
        let copy = filled(*byte);
        sum = crc32_extend(sum, &copy[..]);
        image[(2 + i) * BS..(3 + i) * BS].copy_from_slice(&copy[..]);
    }
    if commit {
        let at = (2 + blocks.len()) * BS;
        image[at..at + BS].fill(0);
        image[at..at + 4].copy_from_slice(&CMAGIC.to_le_bytes());
        image[at + 8..at + 16].copy_from_slice(&sequence.to_le_bytes());
        image[at + 16..at + 20].copy_from_slice(&sum.to_le_bytes());
    }
}

fn clean_header(sequence: u64) -> Vec<u8> {
    let mut h = vec![0; BS];
    h[..4].copy_from_slice(&JMAGIC.to_le_bytes());
    h[8..16].copy_from_slice(&sequence.to_le_bytes());
    h
}

fn mounted(image: Vec<u8>) -> Vec<u8> {
    Fs::mount(MemDisk::from_image(image)).unwrap().into_device().image().to_vec()
}

fn block(image: &[u8], n: usize) -> &[u8] {
    &image[n * BS..(n + 1) * BS]
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Call {
    Write(u64),
    Sync,
}

struct Spy {
    disk: MemDisk,
    calls: Vec<Call>,
}

impl BlockDevice for Spy {
    fn num_blocks(&self) -> u64 {
        self.disk.num_blocks()
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        self.disk.read_block(n, buf)
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        self.calls.push(Call::Write(n));
        self.disk.write_block(n, buf)
    }
    fn sync(&mut self) -> R<()> {
        self.calls.push(Call::Sync);
        self.disk.sync()
    }
}

#[test]
fn a_commit_makes_exactly_these_calls() {
    let mut fs = Fs::mount(Spy { disk: MemDisk::from_image(base()), calls: Vec::new() }).unwrap();
    check!("mounting a clean disk writes nothing", fs.device().calls.clone(), vec![]);
    fs.tx(|fs| {
        for n in [55, 22, 40] {
            fs.write_block(n, &filled(n as u8))?;
        }
        Ok(())
    })
    .unwrap();
    use Call::{Sync, Write};
    // journal starts at block 1: header 1, copies 2 to 4, commit record 5
    let want = vec![
        Write(1), Write(2), Write(3), Write(4), Sync,
        Write(5), Sync,
        Write(22), Write(40), Write(55), Sync,
        Write(1), Sync,
    ];
    check!("calls", std::mem::take(&mut fs.device().calls), want);
    check!("an empty transaction", (fs.tx(|_| Ok(())), fs.device().calls.clone()), (Ok(()), vec![]));
}

#[test]
fn the_journal_holds_what_the_table_says() {
    let mut fs = Fs::mount(MemDisk::from_image(base())).unwrap();
    fs.tx(|fs| {
        fs.write_block(50, &filled(5))?;
        fs.write_block(40, &filled(4))
    })
    .unwrap();
    let after = fs.into_device().image().to_vec();
    // what a commit leaves behind: the copies and the commit record, under a clean header
    let sequence = u64::from_le_bytes(block(&after, 1)[8..16].try_into().unwrap());
    check!("the header is clean", block(&after, 1) == &clean_header(sequence)[..], true);
    let mut want = vec![0; 64 * BS];
    journal(&mut want, sequence, &[(40, 4), (50, 5)], true);
    check!("copies", block(&after, 2) == block(&want, 2) && block(&after, 3) == block(&want, 3), true);
    check!("commit record", block(&after, 4)[..20].to_vec(), block(&want, 4)[..20].to_vec());
}

#[test]
fn sequence_numbers_go_up_and_survive_a_mount() {
    let sequence = |image: &[u8]| u64::from_le_bytes(block(image, 1)[8..16].try_into().unwrap());
    let mut image = base();
    let first = sequence(&image);
    let mut fs = Fs::mount(MemDisk::from_image(image)).unwrap();
    fs.create("/h").unwrap();
    image = fs.into_device().image().to_vec();
    check!("one more", sequence(&image), first + 1);
    journal(&mut image, 500, &[], false);
    let mut fs = Fs::mount(MemDisk::from_image(image)).unwrap();
    fs.create("/i").unwrap();
    check!("goes on from the header", sequence(fs.device().image()), 501);
}

#[test]
fn a_header_with_the_largest_number_is_no_problem() {
    for (count, commit) in [(0usize, false), (1, true), (1, false)] {
        let mut image = base();
        journal(&mut image, u64::MAX, &vec![(50, 1); count], commit);
        let mut fs = Fs::mount(MemDisk::from_image(image)).unwrap();
        check!(format!("{count} copies, commit {commit}: create"), fs.create("/z").is_ok(), true);
        check!("and again", fs.create("/y").is_ok(), true);
        check!("it wrapped", fs.device().image()[BS + 8..BS + 16].to_vec(), 1u64.to_le_bytes().to_vec());
    }
}

#[test]
fn a_commit_record_must_match_in_every_way() {
    let mut sound = base();
    journal(&mut sound, 90, &[(50, 5), (60, 6)], true);
    let record = 4 * BS;
    for (what, at) in [
        ("commit magic", record),
        ("commit number", record + 8),
        ("commit checksum", record + 16),
        ("header number", BS + 8),
        ("a target", BS + 32),
        ("the end of the header", 2 * BS - 1),
        ("the first copy", 2 * BS),
        ("the end of the last copy", 4 * BS - 1),
    ] {
        let mut image = sound.clone();
        image[at] ^= 1;
        let after = mounted(image.clone());
        check!(format!("{what} changed: nothing is replayed"), after[ibm_start() * BS..] == image[ibm_start() * BS..], true);
        check!(format!("{what} changed: journal cleaned"), block(&after, 1)[16..20].to_vec(), vec![0; 4]);
    }
}

fn ibm_start() -> usize {
    17
}

#[test]
fn hostile_headers_replay_nothing() {
    // targets in the superblock, the journal or past the end are refused, and so is the whole transaction
    for bad in [0u64, 1, 5, 16, 64, 1000, u64::from(u32::MAX)] {
        let mut image = base();
        journal(&mut image, 7, &[(50, 5), (bad, 9)], true);
        let after = mounted(image.clone());
        check!(format!("target {bad}"), (after[..BS] == image[..BS], block(&after, 50)[0], block(&after, 1) == &clean_header(7)[..]), (true, 0, true));
    }
    for count in [15u32, 64, 1016, 5000, u32::MAX] {
        let mut image = base();
        journal(&mut image, 7, &[(50, 5)], true);
        image[BS + 16..BS + 20].copy_from_slice(&count.to_le_bytes());
        let after = mounted(image.clone());
        check!(format!("count {count}"), (block(&after, 50)[0], block(&after, 1) == &clean_header(7)[..]), (0, true));
    }
    let mut image = base();
    journal(&mut image, 7, &[(50, 5)], true);
    image[BS] ^= 0xFF;
    check!("no magic: the disk is left as it is", mounted(image.clone()) == image, true);
}

#[test]
fn the_largest_transaction_that_fits_is_replayed() {
    let blocks: Vec<(u64, u8)> = (0..14).map(|i| (40 + i, i as u8 + 1)).collect();
    let mut image = base();
    journal(&mut image, 3, &blocks, true);
    let after = mounted(image);
    for (n, byte) in blocks {
        check!(format!("block {n}"), block(&after, n as usize)[BS - 1], byte);
    }
}

#[test]
fn replaying_twice_gives_the_same_disk() {
    let mut image = base();
    journal(&mut image, 90, &[(50, 5), (60, 6)], true);
    let once = mounted(image);
    check!("mount again", mounted(once.clone()) == once, true);
}

#[test]
fn a_cut_during_recovery_is_harmless() {
    let mut image = base();
    journal(&mut image, 90, &[(50, 5), (60, 6), (61, 7)], true);
    let want = mounted(image.clone());
    for k in 0..6 {
        for seed in 0..25 {
            let mut disk = FaultDisk::from_image(image.clone());
            disk.arm(Some(k));
            let _ = Fs::mount(&mut disk);
            let cut = disk.post_crash_image(if seed == 0 { CrashMode::Prefix } else { CrashMode::Chaos(seed) });
            let after = mounted(cut);
            let same = [50, 60, 61].iter().all(|n| block(&after, *n) == block(&want, *n));
            check!(format!("cut after {k} writes of recovery, seed {seed}"), same, true);
        }
    }
}

#[test]
fn a_commit_that_fails_poisons_the_filesystem() {
    for k in 0..8 {
        let mut fs = Fs::mount(FaultDisk::from_image(base())).unwrap();
        fs.device().arm(Some(k));
        let got = fs.create("/new");
        if got.is_ok() {
            continue;
        }
        check!(format!("cut after {k}"), got, Err(FsError::Crashed));
        check!(format!("cut after {k}: later calls"), fs.readdir("/"), Err(FsError::Poisoned));
        check!(format!("cut after {k}: later changes"), fs.create("/other"), Err(FsError::Poisoned));
    }
}

/// A disk whose n-th write from now fails once. Every other call works.
struct Flaky {
    disk: MemDisk,
    fail_in: Option<u32>,
}

impl BlockDevice for Flaky {
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
                return Err(FsError::Io("once"));
            }
        }
        self.disk.write_block(n, buf)
    }
    fn sync(&mut self) -> R<()> {
        self.disk.sync()
    }
}

#[test]
fn after_one_failed_write_a_remount_finds_old_or_new() {
    let mut reference = Fs::mount(MemDisk::from_image(base())).unwrap();
    let old = snapshot(&mut reference);
    reference.mkdir("/new").unwrap();
    let new = snapshot(&mut reference);
    for k in 0..20 {
        let mut fs = Fs::mount(Flaky { disk: MemDisk::from_image(base()), fail_in: None }).unwrap();
        fs.device().fail_in = Some(k);
        if fs.mkdir("/new").is_ok() {
            continue;
        }
        check!(format!("write {k} failed: no later change"), fs.create("/x"), Err(FsError::Poisoned));
        let got = snapshot(&mut Fs::mount(fs.into_device()).unwrap());
        check!(format!("write {k} failed: old or new"), got == old || got == new, true);
    }
}

type Op = (&'static str, fn(&mut Fs<FaultDisk>) -> R<()>);

#[test]
fn a_cut_at_every_write_of_every_kind_of_call() {
    let ops: [Op; 10] = [
        ("create", |fs| fs.create("/d/new").map(drop)),
        ("mkdir", |fs| fs.mkdir("/d/sub").map(drop)),
        ("grow a file", |fs| fs.write("/d/f", 11 * BS as u64, &[7; 2 * BS])),
        ("overwrite", |fs| fs.write("/d/f", 100, &[8; 5000])),
        ("truncate", |fs| fs.truncate("/d/f", 10)),
        ("unlink", |fs| fs.unlink("/d/f")),
        ("rmdir", |fs| fs.rmdir("/empty")),
        ("link", |fs| fs.link("/d/f", "/h")),
        ("rename", |fs| fs.rename("/d/f", "/g")),
        ("move a directory", |fs| {
            fs.rename("/d", "/e")
        }),
    ];
    let image = base();
    for (name, op) in ops {
        let mut reference = Fs::mount(FaultDisk::from_image(image.clone())).unwrap();
        let old = snapshot(&mut reference);
        reference.device().arm(None);
        op(&mut reference).unwrap();
        let writes = reference.device().writes();
        let new = snapshot(&mut reference);
        let mut outcomes = [0, 0];
        // with `writes` allowed, the last sync is cut; with one more, nothing is
        for k in 0..=writes + 1 {
            let mut fs = Fs::mount(FaultDisk::from_image(image.clone())).unwrap();
            fs.device().arm(Some(k));
            let done = op(&mut fs).is_ok();
            check!(format!("{name}: finishes only when nothing was cut"), done, k > writes);
            for seed in 0..16 {
                let mode = if seed == 0 { CrashMode::Prefix } else { CrashMode::Chaos(seed * 1000 + k) };
                let cut = fs.device().post_crash_image(mode);
                let mut back = Fs::mount(MemDisk::from_image(cut)).unwrap_or_else(|e| panic!("{name}, {k} writes, {mode:?}: mount {e}"));
                let got = snapshot(&mut back);
                check!(format!("{name}: cut after {k} writes, {mode:?}: old or new"), got == old || got == new, true);
                if done {
                    check!(format!("{name}: reported done, {mode:?}: it is there"), got == new, true);
                }
                outcomes[usize::from(got == new)] += 1;
            }
        }
        check!(format!("{name}: both outcomes happen"), outcomes[0] > 0 && outcomes[1] > 0, true);
    }
}

#[test]
fn small_journals_still_take_every_call() {
    let g = Geometry { total_blocks: 128, inode_count: 64, journal_blocks: 12 };
    let mut fs = Fs::mkfs(MemDisk::new(128), &g).unwrap();
    fs.mkdir("/a").unwrap();
    fs.mkdir("/b").unwrap();
    fs.mkdir("/b/old").unwrap();
    fs.mkdir("/a/moved").unwrap();
    fs.create("/a/f").unwrap();
    check!("a write across the indirect boundary", fs.write("/a/f", 10 * BS as u64 + 5, &[1; 30 * BS]), Ok(()));
    check!("a move onto a directory", fs.rename("/a/moved", "/b/old"), Ok(()));
    check!("a shrink", fs.truncate("/a/f", 13 * BS as u64 + 1), Ok(()));
    check!("an unlink", fs.unlink("/a/f"), Ok(()));
}
