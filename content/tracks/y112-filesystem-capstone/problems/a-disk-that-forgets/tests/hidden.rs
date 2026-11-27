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
fn armed_with_zero_the_first_write_fails() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(1, &filled(5)).unwrap();
    disk.arm(Some(0));
    check!("count restarts", disk.writes(), 0);
    check!("write", disk.write_block(2, &filled(1)), Err(FsError::Crashed));
    check!("write again", disk.write_block(2, &filled(1)), Err(FsError::Crashed));
    check!("what was written before arming", sectors(&disk.post_crash_image(CrashMode::Prefix), 1), Some([5; 8]));
    check!("the refused write is nowhere", sectors(&disk.post_crash_image(CrashMode::Prefix), 2), Some([0; 8]));
}

#[test]
fn a_sync_can_be_the_moment_of_the_cut() {
    for k in [0, 2] {
        let mut disk = FaultDisk::new(4);
        disk.arm(Some(k));
        for n in 0..k {
            disk.write_block(n, &filled(n as u8 + 1)).unwrap();
        }
        check!(format!("k = {k}: the sync"), disk.sync(), Err(FsError::Crashed));
        check!(format!("k = {k}: later writes"), disk.write_block(3, &filled(9)), Err(FsError::Crashed));
        if k == 2 {
            let lost = (0..100).any(|seed| sectors(&disk.post_crash_image(CrashMode::Chaos(seed)), 0) == Some([0; 8]));
            check!("the failed sync saved nothing", lost, true);
        }
    }
}

#[test]
fn a_sync_before_the_limit_works() {
    let mut disk = FaultDisk::new(4);
    disk.arm(Some(2));
    disk.write_block(0, &filled(1)).unwrap();
    check!("sync after 1 of 2", disk.sync(), Ok(()));
    check!("second write", disk.write_block(1, &filled(1)), Ok(()));
}

#[test]
fn arming_with_none_only_counts() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(0, &filled(1)).unwrap();
    disk.arm(None);
    for _ in 0..50 {
        disk.write_block(1, &filled(1)).unwrap();
    }
    check!("count", disk.writes(), 50);
}

#[test]
fn bad_block_numbers_are_not_writes() {
    let mut disk = FaultDisk::new(4);
    disk.arm(Some(1));
    for n in [4, 1 << 40, u64::MAX] {
        check!(format!("write {n}"), matches!(disk.write_block(n, &filled(1)), Err(FsError::Io(_))), true);
        check!(format!("read {n}"), matches!(disk.read_block(n, &mut zero_block()), Err(FsError::Io(_))), true);
    }
    check!("count", disk.writes(), 0);
    check!("the one allowed write", disk.write_block(3, &filled(1)), Ok(()));
    disk.post_crash_image(CrashMode::Chaos(0));
}

#[test]
fn looking_at_the_crash_does_not_change_the_disk() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(1, &filled(4)).unwrap();
    for seed in 0..30 {
        disk.post_crash_image(CrashMode::Chaos(seed));
    }
    check!("the running program still sees it", read(&mut disk, 1)[0], 4);
    disk.sync().unwrap();
    check!("and a sync still saves it", sectors(&disk.post_crash_image(CrashMode::Chaos(9)), 1), Some([4; 8]));
}

#[test]
fn from_image_starts_with_everything_safe() {
    let mut image = vec![0; 3 * BS];
    image[BS..2 * BS].fill(6);
    let disk = FaultDisk::from_image(image.clone());
    check!("blocks", disk.num_blocks(), 3);
    check!("same image", disk.post_crash_image(CrashMode::Chaos(5)) == image, true);
}

/// What each of 2000 seeds leaves in the given blocks.
fn outcomes(disk: &FaultDisk, blocks: &[usize]) -> Vec<Vec<Option<[u8; 8]>>> {
    (0..2000)
        .map(|seed| {
            let image = disk.post_crash_image(CrashMode::Chaos(seed));
            blocks.iter().map(|n| sectors(&image, *n)).collect()
        })
        .collect()
}

#[test]
fn one_unsynced_write_may_be_lost_whole_or_torn() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(1, &filled(1)).unwrap();
    disk.sync().unwrap();
    disk.write_block(1, &filled(2)).unwrap();
    let seen: BTreeSet<[u8; 8]> = outcomes(&disk, &[1]).into_iter().map(|o| o[0].expect("a tear inside a sector")).collect();
    check!("every sector is old or new", seen.iter().all(|s| s.iter().all(|b| *b == 1 || *b == 2)), true);
    check!("lost on some seed", seen.contains(&[1; 8]), true);
    check!("whole on some seed", seen.contains(&[2; 8]), true);
    check!("torn on some seed", seen.iter().any(|s| s.contains(&1) && s.contains(&2)), true);
    check!("any sector can be the one that is old or new", (0..8).all(|i| {
        let torn = |s: &&[u8; 8]| s.contains(&1) && s.contains(&2);
        seen.iter().filter(torn).any(|s| s[i] == 1) && seen.iter().filter(torn).any(|s| s[i] == 2)
    }), true);
}

#[test]
fn two_blocks_fail_independently() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(1, &filled(1)).unwrap();
    disk.write_block(2, &filled(2)).unwrap();
    let seen: BTreeSet<(bool, bool)> = outcomes(&disk, &[1, 2])
        .into_iter()
        .filter_map(|o| match (o[0]?, o[1]?) {
            (a, b) if (a == [0; 8] || a == [1; 8]) && (b == [0; 8] || b == [2; 8]) => Some((a == [1; 8], b == [2; 8])),
            _ => None,
        })
        .collect();
    check!("all four of: neither, one, the other, both", seen.len(), 4);
}

#[test]
fn two_writes_to_one_block_may_land_in_either_order() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(1, &filled(1)).unwrap();
    disk.write_block(1, &filled(2)).unwrap();
    let all: Vec<[u8; 8]> = outcomes(&disk, &[1]).into_iter().map(|o| o[0].expect("a tear inside a sector")).collect();
    check!("sectors hold nothing, the first or the second", all.iter().all(|s| s.iter().all(|b| *b <= 2)), true);
    for end in [[0; 8], [1; 8], [2; 8]] {
        check!(format!("some seed ends with {end:?}"), all.contains(&end), true);
    }
    // with both applied whole and in order only, the first could win only when the second is lost
    let first_wins = all.iter().filter(|s| **s == [1; 8]).count();
    let second_wins = all.iter().filter(|s| **s == [2; 8]).count();
    check!(format!("first wins {first_wins} times, second {second_wins}: neither is rare"), first_wins * 2 > second_wins && second_wins * 2 > first_wins, true);
}

#[test]
fn sync_makes_the_latest_content_safe() {
    let mut disk = FaultDisk::new(4);
    disk.write_block(1, &filled(1)).unwrap();
    disk.write_block(1, &filled(2)).unwrap();
    disk.sync().unwrap();
    for seed in 0..200 {
        check!(format!("seed {seed}"), sectors(&disk.post_crash_image(CrashMode::Chaos(seed)), 1), Some([2; 8]));
    }
}

#[test]
fn random_histories_only_leave_what_was_written() {
    let mut rng = anneal_prelude::Rng::new(110_701);
    for round in 0..150 {
        let mut disk = FaultDisk::new(5);
        let mut safe = [0u8; 5];
        let mut pending: Vec<(usize, u8)> = Vec::new();
        for step in 0..rng.below(30) {
            if rng.below(5) == 0 {
                disk.sync().unwrap();
                for (n, byte) in pending.drain(..) {
                    safe[n] = byte;
                }
            } else {
                let n = rng.below(5);
                let byte = step as u8 + 1;
                disk.write_block(n as u64, &filled(byte)).unwrap();
                pending.push((n, byte));
            }
        }
        let seed = rng.next_u64();
        let image = disk.post_crash_image(CrashMode::Chaos(seed));
        let prefix = disk.post_crash_image(CrashMode::Prefix);
        for n in 0..5 {
            let newest = pending.iter().rev().find(|(m, _)| *m == n).map_or(safe[n], |(_, b)| *b);
            check!(format!("round {round} block {n}: prefix"), sectors(&prefix, n), Some([newest; 8]));
            check!(format!("round {round} block {n}: read"), read(&mut disk, n as u64)[BS - 1], newest);
            let got = sectors(&image, n).expect("a tear inside a sector");
            let allowed = |b: u8| b == safe[n] || pending.contains(&(n, b));
            check!(format!("round {round} block {n} seed {seed}: {got:?}"), got.iter().all(|b| allowed(*b)), true);
        }
    }
}

#[test]
fn the_filesystem_runs_on_it() {
    let g = Geometry { total_blocks: 64, inode_count: 16, journal_blocks: 16 };
    let mut fs = Fs::mkfs(FaultDisk::new(64), &g).unwrap();
    fs.create("/f").unwrap();
    fs.write("/f", 0, b"kept").unwrap();
    // every call ended with a sync, so any cut keeps it
    for seed in 0..20 {
        let image = fs.device().post_crash_image(CrashMode::Chaos(seed));
        let mut back = Fs::mount(MemDisk::from_image(image)).unwrap();
        check!(format!("seed {seed}"), back.read("/f", 0, 10), Ok(b"kept".to_vec()));
    }
}
