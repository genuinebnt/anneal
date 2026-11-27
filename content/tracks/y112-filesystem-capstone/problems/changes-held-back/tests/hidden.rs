use solution::*;

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Read(u64),
    Write(u64),
    Sync,
}

/// A disk that writes down every call, and can be told to fail one write.
struct Spy {
    disk: MemDisk,
    calls: Vec<Call>,
    fail_write_to: Option<u64>,
}

impl Spy {
    fn new() -> Spy {
        let mut image = vec![0u8; 64 * BS];
        for n in 0..64 {
            image[n * BS] = n as u8;
        }
        Spy { disk: MemDisk::from_image(image), calls: Vec::new(), fail_write_to: None }
    }
}

impl BlockDevice for Spy {
    fn num_blocks(&self) -> u64 {
        self.disk.num_blocks()
    }
    fn read_block(&mut self, n: u64, buf: &mut Block) -> R<()> {
        self.calls.push(Call::Read(n));
        self.disk.read_block(n, buf)
    }
    fn write_block(&mut self, n: u64, buf: &Block) -> R<()> {
        if self.fail_write_to == Some(n) {
            return Err(FsError::Io("injected"));
        }
        self.calls.push(Call::Write(n));
        self.disk.write_block(n, buf)
    }
    fn sync(&mut self) -> R<()> {
        self.calls.push(Call::Sync);
        self.disk.sync()
    }
}

fn filled(byte: u8) -> Box<Block> {
    let mut b = zero_block();
    b.fill(byte);
    b
}

/// The calls made since the last time this was asked.
fn calls(cache: &mut Cache<Spy>) -> Vec<Call> {
    std::mem::take(&mut cache.device().calls)
}

fn first(cache: &mut Cache<Spy>, n: u64) -> u8 {
    cache.read(n).unwrap()[0]
}

#[test]
fn the_least_recently_used_clean_block_goes_first() {
    let mut cache = Cache::new(Spy::new(), 3);
    for n in [1, 2, 3, 1, 4] {
        first(&mut cache, n);
    }
    calls(&mut cache);
    // 2 was used longest ago
    for n in [1, 3, 4] {
        first(&mut cache, n);
    }
    check!("1, 3 and 4 are still held", calls(&mut cache), vec![]);
    first(&mut cache, 2);
    check!("2 was forgotten", calls(&mut cache), vec![Call::Read(2)]);
    check!("len", cache.len(), 3);
}

#[test]
fn a_write_counts_as_a_use() {
    let mut cache = Cache::new(Spy::new(), 2);
    cache.write(1, &filled(11));
    cache.flush().unwrap();
    first(&mut cache, 2);
    cache.write(1, &filled(12));
    cache.flush().unwrap();
    first(&mut cache, 3); // 2 is now the oldest
    calls(&mut cache);
    check!("block 1", first(&mut cache, 1), 12);
    check!("no disk read", calls(&mut cache), vec![]);
}

#[test]
fn changed_blocks_are_never_forgotten() {
    let mut cache = Cache::new(Spy::new(), 2);
    for n in 0..10 {
        cache.write(n, &filled(100 + n as u8));
    }
    check!("held", cache.len(), 10);
    check!("disk calls", calls(&mut cache), vec![]);
    for n in 0..10 {
        check!(format!("block {n}"), first(&mut cache, n), 100 + n as u8);
    }
    // a clean block has no room: it is read each time
    first(&mut cache, 20);
    first(&mut cache, 20);
    check!("block 20 twice", calls(&mut cache), vec![Call::Read(20), Call::Read(20)]);
    cache.flush().unwrap();
    let want: Vec<Call> = (0..10).map(Call::Write).collect();
    check!("flush writes by rising number, without a sync", calls(&mut cache), want);
    check!("held after flush", cache.len(), 2);
}

#[test]
fn capacity_zero_keeps_only_changes() {
    let mut cache = Cache::new(Spy::new(), 0);
    check!("read", first(&mut cache, 6), 6);
    check!("held", cache.len(), 0);
    cache.write(6, &filled(1));
    check!("held with a change", cache.len(), 1);
    check!("read the change", first(&mut cache, 6), 1);
    cache.flush().unwrap();
    check!("held after flush", cache.len(), 0);
    check!("on the disk", first(&mut cache, 6), 1);
}

#[test]
fn a_failed_read_leaves_nothing_behind() {
    let mut cache = Cache::new(Spy::new(), 4);
    check!("past the end", matches!(cache.read(64), Err(FsError::Io(_))), true);
    check!("held", cache.len(), 0);
}

#[test]
fn a_failed_flush_keeps_what_was_not_written() {
    let mut cache = Cache::new(Spy::new(), 2);
    for n in [1, 2, 3, 4] {
        cache.write(n, &filled(50 + n as u8));
    }
    cache.device().fail_write_to = Some(3);
    check!("flush", cache.flush(), Err(FsError::Io("injected")));
    let left: Vec<u64> = cache.dirty_blocks().iter().map(|(n, _)| *n).collect();
    check!("still to be written", left, vec![3, 4]);
    cache.device().fail_write_to = None;
    calls(&mut cache);
    check!("second flush", cache.flush(), Ok(()));
    check!("writes only the rest", calls(&mut cache), vec![Call::Write(3), Call::Write(4)]);
    let mut disk = cache.into_device().disk;
    let mut b = zero_block();
    for n in 1..5 {
        disk.read_block(n, &mut b).unwrap();
        check!(format!("disk block {n}"), b[0], 50 + n as u8);
    }
}

#[test]
fn random_use_matches_a_simple_model() {
    let mut rng = anneal_prelude::Rng::new(110_201);
    for round in 0..60 {
        let capacity = rng.below(6);
        let mut cache = Cache::new(Spy::new(), capacity);
        let mut disk: Vec<u8> = (0..64).collect();
        // held blocks from the least to the most recently used: (number, first byte, changed)
        let mut held: Vec<(u64, u8, bool)> = Vec::new();
        for step in 0..200 {
            let n = rng.below(12) as u64;
            let mut expected = Vec::new();
            match rng.below(10) {
                0 => {
                    cache.flush().unwrap();
                    let mut changed: Vec<(u64, u8)> = held.iter().filter(|e| e.2).map(|e| (e.0, e.1)).collect();
                    changed.sort();
                    for (n, byte) in changed {
                        disk[n as usize] = byte;
                        expected.push(Call::Write(n));
                    }
                    held.iter_mut().for_each(|e| e.2 = false);
                }
                1 => {
                    cache.discard();
                    held.retain(|e| !e.2);
                }
                2..=4 => {
                    let byte = rng.below(256) as u8;
                    cache.write(n, &filled(byte));
                    // only the first byte matters to the model
                    held.retain(|e| e.0 != n);
                    held.push((n, byte, true));
                }
                _ => {
                    let want = match held.iter().position(|e| e.0 == n) {
                        Some(i) => {
                            let e = held.remove(i);
                            held.push(e);
                            e.1
                        }
                        None => {
                            expected.push(Call::Read(n));
                            held.push((n, disk[n as usize], false));
                            disk[n as usize]
                        }
                    };
                    check!(format!("round {round} step {step}: read {n}"), first(&mut cache, n), want);
                }
            }
            while held.len() > capacity {
                match held.iter().position(|e| !e.2) {
                    Some(i) => held.remove(i),
                    None => break,
                };
            }
            check!(format!("round {round} step {step}, capacity {capacity}: disk calls and size"), (calls(&mut cache), cache.len()), (expected, held.len()));
        }
    }
}

#[test]
fn flush_with_nothing_changed_does_nothing() {
    let mut cache = Cache::new(Spy::new(), 4);
    first(&mut cache, 1);
    calls(&mut cache);
    check!("flush", cache.flush(), Ok(()));
    check!("disk calls", calls(&mut cache), vec![]);
    check!("empty", cache.is_empty(), false);
}
