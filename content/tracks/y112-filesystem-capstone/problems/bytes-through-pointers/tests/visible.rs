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
fn bytes_come_back() {
    let (mut fs, f) = one_file(128);
    check!("write", write(&mut fs, f, 0, b"hello"), Ok(()));
    check!("size", size(&mut fs, f), 5);
    check!("read", read(&mut fs, f, 0, 100), Ok(b"hello".to_vec()));
    check!("read the middle", read(&mut fs, f, 1, 3), Ok(b"ell".to_vec()));
    check!("read past the end", read(&mut fs, f, 5, 3), Ok(vec![]));
}

#[test]
fn a_write_across_a_block_edge_keeps_its_neighbours() {
    let (mut fs, f) = one_file(128);
    write(&mut fs, f, 0, &vec![1; 3 * BS]).unwrap();
    write(&mut fs, f, B - 2, &[9; 4]).unwrap();
    check!("around the edge", read(&mut fs, f, B - 4, 8), Ok(vec![1, 1, 9, 9, 9, 9, 1, 1]));
}

#[test]
fn a_hole_reads_as_zeros_and_costs_nothing() {
    let (mut fs, f) = one_file(128);
    write(&mut fs, f, 5 * B + 7, b"X").unwrap();
    check!("size", size(&mut fs, f), 5 * B + 8);
    check!("blocks owned", usage(&mut fs, f).0, 1);
    check!("the hole", read(&mut fs, f, 0, 5 * BS) == Ok(vec![0; 5 * BS]), true);
    check!("after reading it", usage(&mut fs, f).0, 1);
}

#[test]
fn the_thirteenth_block_needs_a_pointer_block() {
    let (mut fs, f) = one_file(128);
    write(&mut fs, f, 11 * B, b"a").unwrap();
    check!("block 11: data only", usage(&mut fs, f).0, 1);
    write(&mut fs, f, IND * B, b"b").unwrap();
    check!("block 12: data and an indirect block", usage(&mut fs, f).0, 3);
}

#[test]
fn shrinking_gives_blocks_back() {
    let (mut fs, f) = one_file(128);
    let free = fs.free_blocks().unwrap();
    write(&mut fs, f, 0, &vec![7; 20 * BS]).unwrap();
    check!("resize to 0", resize(&mut fs, f, 0), Ok(()));
    check!("blocks owned and free", usage(&mut fs, f), (0, free));
}
