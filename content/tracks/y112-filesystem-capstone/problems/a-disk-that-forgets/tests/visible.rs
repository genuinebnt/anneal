use solution::*;
use std::collections::BTreeSet;

fn filled(byte: u8) -> Box<Block> {
    let mut b = zero_block();
    b.fill(byte);
    b
}

fn read(disk: &mut FaultDisk, n: u64) -> Box<Block> {
    let mut b = zero_block();
    disk.read_block(n, &mut b).unwrap();
    b
}

/// Block `n` of an image, as one byte per sector. `None` if a sector is not all one byte.
fn sectors(image: &[u8], n: usize) -> Option<[u8; 8]> {
    let mut out = [0; 8];
    for (s, sector) in image[n * BS..(n + 1) * BS].chunks(SECTOR).enumerate() {
        if sector.iter().any(|b| *b != sector[0]) {
            return None;
        }
        out[s] = sector[0];
    }
    Some(out)
}

#[test]
fn synced_writes_survive_every_cut() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(2, &filled(7)).unwrap();
    disk.sync().unwrap();
    for mode in [CrashMode::Prefix, CrashMode::Chaos(1), CrashMode::Chaos(2)] {
        check!(format!("{mode:?}"), sectors(&disk.post_crash_image(mode), 2), Some([7; 8]));
    }
}

#[test]
fn a_read_sees_the_newest_write() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(1, &filled(1)).unwrap();
    disk.sync().unwrap();
    disk.write_block(1, &filled(2)).unwrap();
    disk.write_block(1, &filled(3)).unwrap();
    check!("block 1", read(&mut disk, 1)[0], 3);
}

#[test]
fn a_clean_cut_keeps_every_write_in_order() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(1, &filled(1)).unwrap();
    disk.write_block(1, &filled(2)).unwrap();
    disk.write_block(3, &filled(9)).unwrap();
    let image = disk.post_crash_image(CrashMode::Prefix);
    check!("blocks 1 and 3", (sectors(&image, 1), sectors(&image, 3)), (Some([2; 8]), Some([9; 8])));
    check!("four blocks", image.len(), 4 * BS);
}

#[test]
fn an_armed_disk_dies_at_its_write() {
    let mut disk = FaultDisk::new(4);
    disk.arm(Some(2));
    check!("first", disk.write_block(0, &filled(1)), Ok(()));
    check!("second", disk.write_block(1, &filled(1)), Ok(()));
    check!("third", disk.write_block(2, &filled(1)), Err(FsError::Crashed));
    check!("read afterwards", disk.read_block(0, &mut zero_block()), Err(FsError::Crashed));
    check!("sync afterwards", disk.sync(), Err(FsError::Crashed));
    check!("writes that succeeded", disk.writes(), 2);
}

#[test]
fn the_same_seed_gives_the_same_disk() {
    let mut disk = FaultDisk::new(8);
    for n in 0..8 {
        disk.write_block(n, &filled(n as u8 + 1)).unwrap();
    }
    for seed in 0..20 {
        let a = disk.post_crash_image(CrashMode::Chaos(seed));
        check!(format!("seed {seed}"), a == disk.post_crash_image(CrashMode::Chaos(seed)), true);
    }
}
