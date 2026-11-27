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
fn the_second_read_does_not_touch_the_disk() {
    let mut cache = Cache::new(Spy::new(), 4);
    check!("block 5", first(&mut cache, 5), 5);
    check!("block 5 again", first(&mut cache, 5), 5);
    check!("disk calls", calls(&mut cache), vec![Call::Read(5)]);
}

#[test]
fn a_write_waits_for_a_flush() {
    let mut cache = Cache::new(Spy::new(), 4);
    cache.write(7, &filled(70));
    check!("read back", first(&mut cache, 7), 70);
    check!("disk calls before flush", calls(&mut cache), vec![]);
    check!("flush", cache.flush(), Ok(()));
    check!("disk calls of flush", calls(&mut cache), vec![Call::Write(7)]);
    check!("changed blocks left", cache.dirty_blocks().len(), 0);
}

#[test]
fn discard_brings_the_old_content_back() {
    let mut cache = Cache::new(Spy::new(), 4);
    check!("before", first(&mut cache, 3), 3);
    cache.write(3, &filled(99));
    cache.discard();
    check!("after discard", first(&mut cache, 3), 3);
}

#[test]
fn changed_blocks_are_listed_in_order() {
    let mut cache = Cache::new(Spy::new(), 8);
    for n in [9, 2, 30, 2] {
        cache.write(n, &filled(n as u8 + 1));
    }
    let listed: Vec<(u64, u8)> = cache.dirty_blocks().iter().map(|(n, b)| (*n, b[0])).collect();
    check!("dirty_blocks", listed, vec![(2, 3), (9, 10), (30, 31)]);
}

#[test]
fn a_flushed_block_can_be_forgotten() {
    let mut cache = Cache::new(Spy::new(), 0);
    cache.write(1, &filled(8));
    cache.flush().unwrap();
    check!("held", cache.len(), 0);
    check!("read from the disk", first(&mut cache, 1), 8);
}
