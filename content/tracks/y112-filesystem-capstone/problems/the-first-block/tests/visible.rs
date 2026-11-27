use solution::*;

fn geometry(total_blocks: u64, inode_count: u32, journal_blocks: u32) -> Geometry {
    Geometry { total_blocks, inode_count, journal_blocks }
}

#[test]
fn a_block_comes_back_as_it_was_written() {
    let mut disk = MemDisk::new(4);
    let mut block = zero_block();
    block[0] = 7;
    block[BS - 1] = 9;
    check!("write block 2", disk.write_block(2, &block), Ok(()));
    let mut back = zero_block();
    check!("read block 2", disk.read_block(2, &mut back), Ok(()));
    check!("block 2", back == block, true);
    check!("byte 2 * BS of the image", disk.image()[2 * BS], 7);
}

#[test]
fn past_the_end_is_an_error() {
    let mut disk = MemDisk::new(4);
    let mut block = zero_block();
    check!("read block 4 of 4", matches!(disk.read_block(4, &mut block), Err(FsError::Io(_))), true);
    check!("write block 4 of 4", matches!(disk.write_block(4, &block), Err(FsError::Io(_))), true);
}

#[test]
fn the_known_checksum() {
    check!("crc32(123456789)", crc32(b"123456789"), 0xCBF4_3926);
    check!("the same in two calls", crc32_extend(crc32(b"1234"), b"56789"), 0xCBF4_3926);
}

#[test]
fn where_everything_goes() {
    let sb = Super::layout(&geometry(256, 64, 64)).unwrap();
    check!(
        "journal, inode bitmap, data bitmap, inode table, data",
        (sb.journal_start, sb.ibm_start, sb.dbm_start, sb.itable_start, sb.data_start),
        (1, 65, 66, 67, 70)
    );
}

#[test]
fn a_superblock_comes_back() {
    let sb = Super::layout(&geometry(1000, 100, 20)).unwrap();
    check!("decode(encode(sb))", Super::decode(&sb.encode()), Ok(sb));
}

#[test]
fn a_blank_block_is_not_a_superblock() {
    check!("decode(zeros)", Super::decode(&zero_block()), Err(FsError::BadMagic));
}
