use solution::*;

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Write(u64),
    Sync,
}

/// A disk that writes down writes and syncs, and can be told to fail one write.
struct Spy {
    disk: MemDisk,
    calls: Vec<Call>,
    /// The write after this many more fails, once.
    fail_in: Option<u32>,
}

impl Spy {
    fn new(blocks: u64) -> Spy {
        Spy { disk: MemDisk::from_image(vec![0xEE; blocks as usize * BS]), calls: Vec::new(), fail_in: None }
    }
}

impl BlockDevice for Spy {
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
                return Err(FsError::Io("injected"));
            }
        }
        self.calls.push(Call::Write(n));
        self.disk.write_block(n, buf)
    }
    fn sync(&mut self) -> R<()> {
        self.calls.push(Call::Sync);
        self.disk.sync()
    }
}

/// 256 blocks, 32 inodes and a journal of 16: data starts at block 21.
const SMALL: Geometry = Geometry { total_blocks: 256, inode_count: 32, journal_blocks: 16 };

fn new_fs() -> Fs<Spy> {
    Fs::format(Spy::new(256), &SMALL).unwrap()
}

fn image(fs: &mut Fs<Spy>) -> Vec<u8> {
    fs.device().disk.image().to_vec()
}

#[test]
fn a_new_filesystem_has_every_data_block_free() {
    let mut fs = new_fs();
    check!("data starts", fs.superblock().data_start, 21);
    check!("free blocks", fs.free_blocks(), Ok(256 - 21));
}

#[test]
fn blocks_are_handed_out_in_order() {
    let mut fs = new_fs();
    let got = fs.tx(|fs| Ok([fs.alloc_block()?, fs.alloc_block()?, fs.alloc_block()?]));
    check!("three blocks", got, Ok([21, 22, 23]));
    check!("free", fs.free_blocks(), Ok(256 - 24));
}

#[test]
fn an_inode_survives_a_remount() {
    let mut fs = new_fs();
    let ino = fs.tx(|fs| fs.alloc_inode(K_FILE, 1)).unwrap();
    check!("first inode number", ino, 1);
    let mut fs = Fs::mount(fs.into_device()).unwrap();
    let inode = fs.load_inode(ino).unwrap();
    check!("kind, links, mode", (inode.kind, inode.nlink, inode.mode), (K_FILE, 1, 0o644));
}

#[test]
fn a_failed_change_leaves_no_trace() {
    let mut fs = new_fs();
    let before = image(&mut fs);
    let result: R<()> = fs.tx(|fs| {
        fs.alloc_inode(K_DIR, 2)?;
        fs.alloc_block()?;
        Err(FsError::NotEmpty)
    });
    check!("result", result, Err(FsError::NotEmpty));
    check!("the disk is unchanged", image(&mut fs) == before, true);
    check!("free blocks", fs.free_blocks(), Ok(256 - 21));
}

#[test]
fn a_device_with_nothing_on_it_cannot_be_mounted() {
    check!("mount a blank disk", Fs::mount(MemDisk::new(256)).err(), Some(FsError::BadMagic));
}
