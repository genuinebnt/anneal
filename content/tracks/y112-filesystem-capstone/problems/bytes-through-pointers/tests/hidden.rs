use solution::*;
use std::collections::{BTreeMap, BTreeSet};

const IND: u64 = 12;
const DOUBLE: u64 = 12 + 1024;
const B: u64 = BS as u64;

/// A filesystem with one empty file, and the file's inode number.
fn one_file(total_blocks: u64) -> (Fs<MemDisk>, u32) {
    let g = Geometry { total_blocks, inode_count: 16, journal_blocks: 16 };
    let mut fs = Fs::format(MemDisk::new(total_blocks), &g).unwrap();
    let ino = fs.tx(|fs| fs.alloc_inode(K_FILE, 1)).unwrap();
    (fs, ino)
}

fn write(fs: &mut Fs<MemDisk>, ino: u32, offset: u64, data: &[u8]) -> R<()> {
    fs.tx(|fs| {
        let mut inode = fs.load_inode(ino)?;
        fs.write_at(ino, &mut inode, offset, data)
    })
}

fn resize(fs: &mut Fs<MemDisk>, ino: u32, size: u64) -> R<()> {
    fs.tx(|fs| {
        let mut inode = fs.load_inode(ino)?;
        fs.resize(ino, &mut inode, size)
    })
}

fn read(fs: &mut Fs<MemDisk>, ino: u32, offset: u64, len: usize) -> R<Vec<u8>> {
    let inode = fs.load_inode(ino)?;
    fs.read_at(&inode, offset, len)
}

fn size(fs: &mut Fs<MemDisk>, ino: u32) -> u64 {
    fs.load_inode(ino).unwrap().size
}

/// (blocks the file owns, free data blocks)
fn usage(fs: &mut Fs<MemDisk>, ino: u32) -> (usize, u64) {
    let inode = fs.load_inode(ino).unwrap();
    (fs.file_blocks(&inode).unwrap().len(), fs.free_blocks().unwrap())
}

#[test]
fn every_region_costs_what_it_should() {
    let (mut fs, f) = one_file(256);
    // (file block, blocks owned after writing one byte there)
    let steps = [
        (0, 1),
        (11, 2),
        (IND, 4),              // indirect + data
        (DOUBLE - 1, 5),       // data
        (DOUBLE, 8),           // double + middle + data
        (DOUBLE + 1023, 9),    // data
        (DOUBLE + 1024, 11),   // second middle + data
        (MAX_FILE_BLOCKS - 1, 13),
    ];
    for (block, owned) in steps {
        write(&mut fs, f, block * B + 17, &[block as u8 | 1; 3]).unwrap();
        check!(format!("after file block {block}"), usage(&mut fs, f).0, owned);
    }
    for (block, _) in steps {
        check!(format!("file block {block}"), read(&mut fs, f, block * B + 17, 3), Ok(vec![block as u8 | 1; 3]));
    }
    let blocks = fs.load_inode(f).map(|i| (i.direct[0] != 0, i.direct[11] != 0, i.indirect != 0, i.double != 0));
    check!("which fields are used", blocks, Ok((true, true, true, true)));
}

#[test]
fn block_of_places_pointers_in_their_slots() {
    let (mut fs, f) = one_file(128);
    let found = fs.tx(|fs| {
        let mut inode = fs.load_inode(f)?;
        let direct = fs.block_of(&mut inode, 3, true)?;
        let single = fs.block_of(&mut inode, IND + 5, true)?;
        let double = fs.block_of(&mut inode, DOUBLE + 1024 * 2 + 9, true)?;
        let slot = |fs: &mut Fs<MemDisk>, block: u32, i: usize| -> R<u32> {
            let b = fs.read_block(u64::from(block))?;
            Ok(u32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap()))
        };
        let from_single = slot(fs, inode.indirect, 5)?;
        let middle = slot(fs, inode.double, 2)?;
        let from_double = slot(fs, middle, 9)?;
        let again = fs.block_of(&mut inode, DOUBLE + 1024 * 2 + 9, false)?;
        Ok((inode.direct[3] == direct, from_single == single, from_double == double, again == double))
    });
    check!("direct, indirect, double, asked again", found, Ok((true, true, true, true)));
}

#[test]
fn asking_without_allocating_changes_nothing() {
    let (mut fs, f) = one_file(128);
    let free = fs.free_blocks().unwrap();
    let mut inode = fs.load_inode(f).unwrap();
    for block in [0, 11, IND, DOUBLE, DOUBLE + 5000, MAX_FILE_BLOCKS - 1] {
        check!(format!("block {block}"), fs.block_of(&mut inode, block, false), Ok(0));
    }
    check!("inode and free blocks", (inode, fs.free_blocks()), (fs.load_inode(f).unwrap(), Ok(free)));
    for block in [MAX_FILE_BLOCKS, u64::MAX] {
        let mut i = Inode::default();
        check!(format!("block {block}"), fs.block_of(&mut i, block, false), Err(FsError::FileTooBig));
    }
}

#[test]
fn the_largest_file_and_one_byte_more() {
    let (mut fs, f) = one_file(128);
    check!("the last byte", write(&mut fs, f, MAX_FILE_SIZE - 1, &[0x5A]), Ok(()));
    check!("read it", read(&mut fs, f, MAX_FILE_SIZE - 1, 10), Ok(vec![0x5A]));
    let free = fs.free_blocks().unwrap();
    for (offset, len) in [(MAX_FILE_SIZE, 1), (MAX_FILE_SIZE - 1, 2), (u64::MAX, 2), (u64::MAX - 3, 3)] {
        check!(format!("write {len} at {offset}"), write(&mut fs, f, offset, &vec![1; len]), Err(FsError::FileTooBig));
    }
    check!("resize past the limit", resize(&mut fs, f, MAX_FILE_SIZE + 1), Err(FsError::FileTooBig));
    check!("nothing changed", (size(&mut fs, f), fs.free_blocks()), (MAX_FILE_SIZE, Ok(free)));
    check!("reading far past the end", read(&mut fs, f, u64::MAX, 10), Ok(vec![]));
    check!("shrink to 1", resize(&mut fs, f, 1), Ok(()));
    check!("all is free again", usage(&mut fs, f).0, 0);
}

#[test]
fn an_empty_write_does_nothing_at_all() {
    let (mut fs, f) = one_file(128);
    let before = fs.load_inode(f).unwrap();
    check!("write nothing far out", write(&mut fs, f, 100 * B, b""), Ok(()));
    check!("inode", fs.load_inode(f), Ok(before));
}

#[test]
fn times_change_with_the_file() {
    let (mut fs, f) = one_file(128);
    let t0 = fs.load_inode(f).unwrap();
    write(&mut fs, f, 0, b"abc").unwrap();
    let t1 = fs.load_inode(f).unwrap();
    resize(&mut fs, f, 100).unwrap();
    let t2 = fs.load_inode(f).unwrap();
    resize(&mut fs, f, 100).unwrap();
    let t3 = fs.load_inode(f).unwrap();
    check!("write sets both", t1.mtime > t0.mtime && t1.mtime == t1.ctime, true);
    check!("growing sets both", t2.mtime > t1.mtime && t2.mtime == t2.ctime, true);
    check!("the same size sets both", t3.mtime > t2.mtime && t3.mtime == t3.ctime, true);
}

#[test]
fn growing_after_shrinking_reads_zeros() {
    let (mut fs, f) = one_file(128);
    write(&mut fs, f, 0, &vec![0xAB; 6000]).unwrap();
    resize(&mut fs, f, 100).unwrap();
    resize(&mut fs, f, 6000).unwrap();
    let back = read(&mut fs, f, 0, 7000).unwrap();
    check!("length", back.len(), 6000);
    check!("kept, then zeros", back[..100].iter().all(|b| *b == 0xAB) && back[100..].iter().all(|b| *b == 0), true);
    check!("one block", usage(&mut fs, f).0, 1);
}

#[test]
fn a_pointer_block_goes_only_when_all_it_covers_is_cut() {
    let (mut fs, f) = one_file(256);
    write(&mut fs, f, (IND + 500) * B, b"a").unwrap();
    write(&mut fs, f, (DOUBLE + 1024 + 700) * B, b"b").unwrap();
    check!("owned", usage(&mut fs, f).0, 5);
    // cut inside what the second middle block covers: it stays, empty
    resize(&mut fs, f, (DOUBLE + 1024 + 1) * B).unwrap();
    check!("second middle block stays", usage(&mut fs, f).0, 4);
    // cut at its first block: now it goes
    resize(&mut fs, f, (DOUBLE + 1024) * B).unwrap();
    check!("second middle block goes", usage(&mut fs, f).0, 3);
    resize(&mut fs, f, DOUBLE * B + 1).unwrap();
    check!("double stays", usage(&mut fs, f).0, 3);
    resize(&mut fs, f, DOUBLE * B).unwrap();
    check!("double goes, the indirect part stays", usage(&mut fs, f).0, 2);
    resize(&mut fs, f, (IND + 1) * B).unwrap();
    check!("indirect stays", usage(&mut fs, f).0, 1);
    resize(&mut fs, f, IND * B).unwrap();
    check!("indirect goes", usage(&mut fs, f).0, 0);
    check!("fields cleared", fs.load_inode(f).map(|i| (i.indirect, i.double)), Ok((0, 0)));
}

#[test]
fn a_cut_slot_is_cleared_in_the_pointer_block() {
    let (mut fs, f) = one_file(128);
    write(&mut fs, f, (IND + 1) * B, b"a").unwrap();
    write(&mut fs, f, (IND + 2) * B, b"b").unwrap();
    resize(&mut fs, f, (IND + 2) * B).unwrap();
    resize(&mut fs, f, (IND + 3) * B).unwrap();
    check!("the cut block reads as a hole", read(&mut fs, f, (IND + 2) * B, 1), Ok(vec![0]));
    check!("the kept one", read(&mut fs, f, (IND + 1) * B, 1), Ok(b"a".to_vec()));
}

#[test]
fn a_full_disk_leaves_the_file_as_it_was() {
    let (mut fs, f) = one_file(48);
    write(&mut fs, f, 0, &vec![3; 2 * BS]).unwrap();
    let before = (fs.load_inode(f).unwrap(), usage(&mut fs, f));
    check!("too much", write(&mut fs, f, 2 * B, &vec![4; 60 * BS]), Err(FsError::NoSpace));
    check!("unchanged", (fs.load_inode(f).unwrap(), usage(&mut fs, f)), before);
    let rest = fs.free_blocks().unwrap() as usize;
    check!("exactly what is left, less one pointer block", write(&mut fs, f, 2 * B, &vec![4; 10 * BS + (rest - 11) * BS]), Ok(()));
    check!("now full", fs.free_blocks(), Ok(0));
}

#[test]
fn hostile_pointers_are_errors() {
    for target in [1u32, 5, 17, 128, u32::MAX] {
        let (mut fs, f) = one_file(128);
        write(&mut fs, f, 0, b"x").unwrap();
        write(&mut fs, f, IND * B, b"y").unwrap();
        let mut inode = fs.load_inode(f).unwrap();
        let sound = inode.clone();
        // a direct pointer
        inode.direct[0] = target;
        fs.tx(|fs| fs.store_inode(f, &inode)).unwrap();
        let corrupt = |r: R<Vec<u8>>| matches!(r, Err(FsError::Corrupt(_)));
        check!(format!("direct -> {target}: read"), corrupt(read(&mut fs, f, 0, 1)), true);
        check!(format!("direct -> {target}: write"), matches!(write(&mut fs, f, 1, b"z"), Err(FsError::Corrupt(_))), true);
        check!(format!("direct -> {target}: list"), matches!(fs.file_blocks(&inode), Err(FsError::Corrupt(_))), true);
        check!(format!("direct -> {target}: cut"), matches!(resize(&mut fs, f, 0), Err(FsError::Corrupt(_))), true);
        // a slot inside the pointer block
        fs.tx(|fs| {
            fs.store_inode(f, &sound)?;
            let mut b = fs.read_block(u64::from(sound.indirect))?;
            b[..4].copy_from_slice(&target.to_le_bytes());
            fs.write_block(u64::from(sound.indirect), &b)
        })
        .unwrap();
        check!(format!("slot -> {target}: read"), corrupt(read(&mut fs, f, IND * B, 1)), true);
        check!(format!("slot -> {target}: cut"), matches!(resize(&mut fs, f, 0), Err(FsError::Corrupt(_))), true);
        // the pointer block itself
        let broken = Inode { indirect: target, ..sound };
        fs.tx(|fs| fs.store_inode(f, &broken)).unwrap();
        check!(format!("indirect = {target}: read"), corrupt(read(&mut fs, f, IND * B, 1)), true);
        check!(format!("indirect = {target}: list"), matches!(fs.file_blocks(&broken), Err(FsError::Corrupt(_))), true);
    }
}

#[test]
fn pointer_blocks_that_repeat_each_other_are_refused() {
    let (mut fs, f) = one_file(128);
    // every slot of 100 names 101, and every slot of 101 names 102: a million pointers on a 128 block disk
    let mut inode = fs.load_inode(f).unwrap();
    inode.double = 100;
    inode.size = MAX_FILE_SIZE;
    fs.tx(|fs| {
        for (block, target) in [(100u64, 101u32), (101, 102)] {
            let mut b = zero_block();
            for slot in b.chunks_exact_mut(4) {
                slot.copy_from_slice(&target.to_le_bytes());
            }
            fs.write_block(block, &b)?;
        }
        fs.store_inode(f, &inode)
    })
    .unwrap();
    check!("file_blocks", matches!(fs.file_blocks(&inode), Err(FsError::Corrupt(_))), true);
}

#[test]
fn random_writes_and_cuts_match_a_sparse_model() {
    let mut rng = anneal_prelude::Rng::new(110_401);
    // offsets are drawn around the places where the mapping changes
    let places = [0, 11 * B, IND * B, (IND + 1023) * B, DOUBLE * B, (DOUBLE + 1024) * B, (DOUBLE + 2047) * B];
    for round in 0..6 {
        let (mut fs, f) = one_file(512);
        let free = fs.free_blocks().unwrap();
        let mut size_now = 0u64;
        let mut data: BTreeMap<u64, Vec<u8>> = BTreeMap::new(); // file block -> content
        let mut pointers: BTreeSet<u64> = BTreeSet::new(); // first file block each pointer block covers
        let mut double = false;
        for step in 0..60 {
            let place = places[rng.below(places.len())];
            let offset = (place + rng.below(3 * BS) as u64).saturating_sub(BS as u64);
            if rng.below(4) == 0 {
                let new = if rng.below(6) == 0 { 0 } else { offset };
                check!(format!("round {round} step {step}: resize {new}"), resize(&mut fs, f, new), Ok(()));
                if new < size_now {
                    let keep = new.div_ceil(B);
                    data.retain(|i, _| *i < keep);
                    pointers.retain(|first| *first < keep);
                    double &= DOUBLE < keep;
                    if new % B != 0 {
                        if let Some(block) = data.get_mut(&(new / B)) {
                            block[(new % B) as usize..].fill(0);
                        }
                    }
                }
                size_now = new;
            } else {
                let len = 1 + rng.below(2 * BS);
                let byte = 1 + rng.below(250) as u8;
                check!(format!("round {round} step {step}: write {len} at {offset}"), write(&mut fs, f, offset, &vec![byte; len]), Ok(()));
                for at in offset..offset + len as u64 {
                    let i = at / B;
                    data.entry(i).or_insert_with(|| vec![0; BS])[(at % B) as usize] = byte;
                    if i >= DOUBLE {
                        double = true;
                        pointers.insert(DOUBLE + (i - DOUBLE) / 1024 * 1024);
                    } else if i >= IND {
                        pointers.insert(IND);
                    }
                }
                size_now = size_now.max(offset + len as u64);
            }
            let from = (places[rng.below(places.len())] + rng.below(BS) as u64).saturating_sub(300);
            let want: Vec<u8> = (from..(from + 600).min(size_now.max(from)))
                .map(|at| data.get(&(at / B)).map_or(0, |b| b[(at % B) as usize]))
                .collect();
            check!(format!("round {round} step {step}: read at {from}"), read(&mut fs, f, from, 600), Ok(want));
            let owned = data.len() + pointers.len() + usize::from(double);
            check!(format!("round {round} step {step}: size, owned, free"), (size(&mut fs, f), usage(&mut fs, f)), (size_now, (owned, free - owned as u64)));
        }
    }
}
