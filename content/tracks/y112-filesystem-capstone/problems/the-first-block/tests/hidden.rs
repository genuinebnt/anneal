use anneal_prelude::Rng;
use solution::*;

fn geometry(total_blocks: u64, inode_count: u32, journal_blocks: u32) -> Geometry {
    Geometry { total_blocks, inode_count, journal_blocks }
}

/// CRC-32 one bit at a time: the oracle.
fn slow_crc(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &byte in data {
        c ^= u32::from(byte);
        for _ in 0..8 {
            c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// Puts a correct checksum back after a field was changed.
fn reseal(block: &mut Block) {
    let sum = slow_crc(&block[..72]);
    block[72..76].copy_from_slice(&sum.to_le_bytes());
}

fn put(block: &mut Block, at: usize, bytes: &[u8]) {
    block[at..at + bytes.len()].copy_from_slice(bytes);
}

#[test]
fn checksum_matches_a_bit_at_a_time_in_any_pieces() {
    let mut rng = Rng::new(110_101);
    for _ in 0..200 {
        let len = rng.below(300);
        let data: Vec<u8> = rng.vec(len, 0, 255);
        let cut = rng.below(len + 1);
        let cut2 = cut + rng.below(len - cut + 1);
        let pieces = crc32_extend(crc32_extend(crc32(&data[..cut]), &data[cut..cut2]), &data[cut2..]);
        check!(format!("{len} bytes cut at {cut} and {cut2}"), (crc32(&data), pieces), (slow_crc(&data), slow_crc(&data)));
    }
    check!("nothing", crc32(b""), 0);
}

#[test]
fn the_superblock_bytes_are_where_the_table_says() {
    let sb = Super::layout(&geometry(100_000, 200, 30)).unwrap();
    let mut want = zero_block();
    put(&mut want, 0, &MAGIC.to_le_bytes());
    put(&mut want, 8, &1u32.to_le_bytes());
    put(&mut want, 12, &4096u32.to_le_bytes());
    put(&mut want, 16, &100_000u64.to_le_bytes());
    put(&mut want, 24, &200u32.to_le_bytes());
    put(&mut want, 28, &30u32.to_le_bytes());
    // 100 000 blocks need 4 data bitmap blocks; 201 inode records need 7 table blocks
    for (at, v) in [(32, 1u64), (40, 31), (48, 32), (56, 36), (64, 43)] {
        put(&mut want, at, &v.to_le_bytes());
    }
    reseal(&mut want);
    let got = sb.encode();
    check!("bytes 0..76", got[..76].to_vec(), want[..76].to_vec());
    check!("the rest is zero", got[76..].iter().all(|b| *b == 0), true);
}

#[test]
fn layout_limits() {
    let bad = Err(FsError::BadGeometry);
    check!("no inodes", Super::layout(&geometry(256, 0, 16)), bad.clone());
    check!("32768 inodes", Super::layout(&geometry(100_000, 32768, 16)), bad.clone());
    check!("32767 inodes", Super::layout(&geometry(100_000, 32767, 16)).is_ok(), true);
    check!("journal of 11", Super::layout(&geometry(256, 8, 11)), bad.clone());
    check!("journal of 12", Super::layout(&geometry(256, 8, 12)).is_ok(), true);
    check!("journal of 1000", Super::layout(&geometry(5000, 8, 1000)).is_ok(), true);
    check!("journal of 1001", Super::layout(&geometry(5000, 8, 1001)), bad.clone());
    // with 8 inodes and a journal of 12, the data area starts at block 16
    check!("7 data blocks", Super::layout(&geometry(23, 8, 12)), bad.clone());
    check!("8 data blocks", Super::layout(&geometry(24, 8, 12)).map(|s| s.data_start), Ok(16));
    check!("u32::MAX blocks", Super::layout(&geometry(u64::from(u32::MAX), 8, 12)).is_ok(), true);
    check!("more than u32::MAX blocks", Super::layout(&geometry(u64::from(u32::MAX) + 1, 8, 12)), bad.clone());
    check!("u64::MAX blocks", Super::layout(&geometry(u64::MAX, 8, 12)), bad.clone());
    check!("u32::MAX inodes and journal", Super::layout(&geometry(u64::MAX, u32::MAX, u32::MAX)), bad);
}

#[test]
fn one_inode_block_or_two() {
    // 31 inodes and the unused record 0 fill one block of 32 records
    let one = Super::layout(&geometry(500, 31, 12)).unwrap();
    let two = Super::layout(&geometry(500, 32, 12)).unwrap();
    check!("31 inodes", one.data_start - one.itable_start, 1);
    check!("32 inodes", two.data_start - two.itable_start, 2);
    let small = Super::layout(&geometry(32768, 8, 12)).unwrap();
    let large = Super::layout(&geometry(32769, 8, 12)).unwrap();
    check!("32768 blocks: bitmap blocks", small.itable_start - small.dbm_start, 1);
    check!("32769 blocks: bitmap blocks", large.itable_start - large.dbm_start, 2);
}

#[test]
fn every_flipped_bit_in_the_first_76_bytes_is_refused() {
    let good = Super::layout(&geometry(4096, 128, 64)).unwrap().encode();
    for at in 0..76 {
        for bit in 0..8 {
            let mut bad = good.clone();
            bad[at] ^= 1 << bit;
            let want = if at < 8 { FsError::BadMagic } else { FsError::BadChecksum };
            check!(format!("byte {at} bit {bit}"), Super::decode(&bad), Err(want));
        }
    }
    let mut after = good.clone();
    after[76] = 1;
    after[BS - 1] = 1;
    check!("bytes after 76 are not read", Super::decode(&after).is_ok(), true);
}

#[test]
fn a_sealed_block_must_still_make_sense() {
    let good = Super::layout(&geometry(4096, 128, 64)).unwrap().encode();
    let cases: [(&str, usize, Vec<u8>); 9] = [
        ("version 2", 8, 2u32.to_le_bytes().to_vec()),
        ("block size 512", 12, 512u32.to_le_bytes().to_vec()),
        ("u64::MAX blocks", 16, u64::MAX.to_le_bytes().to_vec()),
        ("no inodes", 24, 0u32.to_le_bytes().to_vec()),
        ("huge journal", 28, u32::MAX.to_le_bytes().to_vec()),
        ("journal elsewhere", 32, 2u64.to_le_bytes().to_vec()),
        ("inode bitmap elsewhere", 40, 0u64.to_le_bytes().to_vec()),
        ("inode table elsewhere", 56, 3u64.to_le_bytes().to_vec()),
        ("data elsewhere", 64, 1u64.to_le_bytes().to_vec()),
    ];
    for (what, at, bytes) in cases {
        let mut bad = good.clone();
        put(&mut bad, at, &bytes);
        reseal(&mut bad);
        check!(what, Super::decode(&bad), Err(FsError::BadGeometry));
    }
}

#[test]
fn random_geometries_round_trip_and_do_not_overlap() {
    let mut rng = Rng::new(110_102);
    for _ in 0..500 {
        let g = geometry(rng.int(0, 200_000) as u64, rng.int(0, 40_000) as u32, rng.int(0, 1100) as u32);
        if let Ok(sb) = Super::layout(&g) {
            let ordered = 1 == sb.journal_start
                && sb.journal_start + u64::from(g.journal_blocks) == sb.ibm_start
                && sb.ibm_start + 1 == sb.dbm_start
                && (sb.itable_start - sb.dbm_start) * 32768 >= g.total_blocks
                && (sb.data_start - sb.itable_start) * 32 > u64::from(g.inode_count)
                && sb.data_start + 8 <= g.total_blocks;
            check!(format!("{g:?}"), (ordered, Super::decode(&sb.encode())), (true, Ok(sb)));
        } else {
            let data_start = 1 + u64::from(g.journal_blocks) + 1 + g.total_blocks.div_ceil(32768).max(1) + (u64::from(g.inode_count) + 1).div_ceil(32);
            let fine = (1..32768).contains(&g.inode_count) && (12..=1000).contains(&g.journal_blocks) && g.total_blocks >= data_start + 8;
            check!(format!("refused {g:?}"), fine, false);
        }
    }
}

#[test]
fn the_memory_disk_refuses_numbers_far_past_the_end() {
    let mut disk = MemDisk::new(2);
    let mut block = zero_block();
    // 2^52 - 1 times 4096 is the last multiple of 4096 that fits in 64 bits
    for n in [2, 3, 1 << 31, 1 << 40, (1u64 << 52) - 1, 1 << 52, u64::MAX] {
        check!(format!("read {n}"), matches!(disk.read_block(n, &mut block), Err(FsError::Io(_))), true);
        check!(format!("write {n}"), matches!(disk.write_block(n, &block), Err(FsError::Io(_))), true);
    }
    check!("sync", disk.sync(), Ok(()));
}

#[test]
fn an_image_with_a_ragged_end() {
    let mut disk = MemDisk::from_image(vec![1; 2 * BS + 100]);
    check!("whole blocks", disk.num_blocks(), 2);
    let mut block = zero_block();
    check!("block 2 is not whole", matches!(disk.read_block(2, &mut block), Err(FsError::Io(_))), true);
    check!("the image is unchanged", disk.image().len(), 2 * BS + 100);
}
