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

#[test]
fn a_committed_transaction_is_finished_at_mount() {
    let mut image = base();
    journal(&mut image, 90, &[(50, 5), (60, 6)], true);
    let after = mounted(image);
    check!("blocks 50 and 60", (block(&after, 50)[100], block(&after, 60)[100]), (5, 6));
    check!("the journal is clean and keeps the number", block(&after, 1) == &clean_header(90)[..], true);
}

#[test]
fn without_a_commit_record_nothing_happens() {
    let mut image = base();
    journal(&mut image, 90, &[(50, 5), (60, 6)], false);
    let after = mounted(image);
    check!("blocks 50 and 60", (block(&after, 50)[100], block(&after, 60)[100]), (0, 0));
    check!("the journal is clean", block(&after, 1) == &clean_header(90)[..], true);
}

#[test]
fn a_transaction_too_large_for_the_journal_is_refused() {
    let mut fs = Fs::mount(MemDisk::from_image(base())).unwrap();
    let before = fs.device().image().to_vec();
    let got = fs.tx(|fs| (30..45).try_for_each(|n| fs.write_block(n, &filled(1))));
    check!("15 blocks, 14 fit", got, Err(FsError::TxTooBig));
    check!("the disk is untouched", fs.device().image() == &before[..], true);
    check!("and the filesystem goes on", fs.tx(|fs| (30..44).try_for_each(|n| fs.write_block(n, &filled(1)))), Ok(()));
}

#[test]
fn a_cut_during_create_gives_before_or_after() {
    let image = base();
    let mut before = Fs::mount(MemDisk::from_image(image.clone())).unwrap();
    let old = snapshot(&mut before);
    before.create("/new").unwrap();
    let new = snapshot(&mut before);
    for k in 0..40 {
        let mut fs = Fs::mount(FaultDisk::from_image(image.clone())).unwrap();
        fs.device().arm(Some(k));
        let done = fs.create("/new").is_ok();
        let cut = fs.device().post_crash_image(CrashMode::Prefix);
        let got = snapshot(&mut Fs::mount(MemDisk::from_image(cut)).unwrap());
        check!(format!("after {k} writes: old or new"), got == old || got == new, true);
        if done {
            check!(format!("after {k} writes: it said done"), got == new, true);
        }
    }
}

#[test]
fn an_image_in_a_real_file() {
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let path = Temp(std::env::temp_dir().join(format!("anneal-toyfs-{}.img", std::process::id())));
    let _ = std::fs::remove_file(&path.0);
    let mut fs = Fs::mkfs(FileDevice::create(&path.0, 64).unwrap(), &G).unwrap();
    fs.mkdir("/docs").unwrap();
    fs.create("/docs/a").unwrap();
    fs.write("/docs/a", 100_000, b"tail").unwrap();
    drop(fs);
    let mut fs = Fs::mount(FileDevice::open(&path.0).unwrap()).unwrap();
    check!("read after reopening", fs.read("/docs/a", 100_000, 10), Ok(b"tail".to_vec()));
}
