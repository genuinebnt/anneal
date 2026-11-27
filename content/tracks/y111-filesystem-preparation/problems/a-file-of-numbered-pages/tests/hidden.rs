use solution::*;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// A new folder of its own, removed at the end.
struct Folder(PathBuf);

impl Folder {
    fn new() -> Folder {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let kind = module_path!().replace(':', "-");
        let path = std::env::temp_dir().join(format!("anneal-pages-{kind}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Folder(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn read(file: &PageFile, n: u64, size: usize) -> Result<Vec<u8>, PageError> {
    let mut page = vec![0xCC; size];
    file.read(n, &mut page).map(|()| page)
}

#[test]
fn bad_page_sizes_and_lengths() {
    let folder = Folder::new();
    check!("create with size 0", PageFile::create(&folder.file("z"), 0, 4).map(|_| ()), Err(PageError::BadPageSize));
    check!("no file was made", folder.file("z").exists(), false);
    std::fs::write(folder.file("odd"), [1; 20]).unwrap();
    check!("open with size 0", PageFile::open(&folder.file("odd"), 0).map(|_| ()), Err(PageError::BadPageSize));
    check!("20 bytes in pages of 8", PageFile::open(&folder.file("odd"), 8).map(|_| ()), Err(PageError::NotWholePages));
    check!("20 bytes in pages of 10", PageFile::open(&folder.file("odd"), 10).map(|f| f.pages()), Ok(2));
    check!("page larger than the file", PageFile::open(&folder.file("odd"), 40).map(|_| ()), Err(PageError::NotWholePages));
    check!("the file is as it was", std::fs::read(folder.file("odd")).unwrap(), vec![1; 20]);
}

#[test]
fn opening_what_is_not_there() {
    let folder = Folder::new();
    check!("open", PageFile::open(&folder.file("no"), 8).map(|_| ()), Err(PageError::Io(ErrorKind::NotFound)));
    check!("still not there", folder.file("no").exists(), false);
}

#[test]
fn sizes_that_do_not_fit_make_no_file() {
    let folder = Folder::new();
    for (size, pages) in [(8, u64::MAX), (4096, 1u64 << 52), (2, 1 << 63), (usize::MAX, 2), (1, i64::MAX as u64 + 1)] {
        let got = PageFile::create(&folder.file("big"), size, pages).map(|_| ());
        check!(format!("{pages} pages of {size}"), got, Err(PageError::OutOfRange));
        check!(format!("{pages} pages of {size}: no file"), folder.file("big").exists(), false);
    }
}

#[test]
fn huge_page_numbers() {
    let folder = Folder::new();
    let file = PageFile::create(&folder.file("a"), 4096, 2).unwrap();
    for n in [2, 3, 1 << 40, 1 << 52, u64::MAX] {
        check!(format!("read {n}"), read(&file, n, 4096), Err(PageError::OutOfRange));
        check!(format!("write {n}"), file.write(n, &[1; 4096]), Err(PageError::OutOfRange));
    }
}

#[test]
fn a_buffer_must_be_one_page() {
    let folder = Folder::new();
    let file = PageFile::create(&folder.file("a"), 8, 2).unwrap();
    for len in [0, 7, 9, 16] {
        check!(format!("read {len}"), read(&file, 0, len), Err(PageError::WrongLength));
        check!(format!("write {len}"), file.write(0, &vec![1; len]), Err(PageError::WrongLength));
        // the length is judged before the number
        check!(format!("write {len} past the end"), file.write(50, &vec![1; len]), Err(PageError::WrongLength));
    }
    check!("nothing written", std::fs::read(folder.file("a")).unwrap(), vec![0; 16]);
}

#[test]
fn growing_adds_zero_pages() {
    let folder = Folder::new();
    let mut file = PageFile::create(&folder.file("a"), 8, 0).unwrap();
    check!("empty", read(&file, 0, 8), Err(PageError::OutOfRange));
    check!("first new page", file.grow(2), Ok(0));
    file.write(1, &[5; 8]).unwrap();
    check!("next new page", file.grow(3), Ok(2));
    check!("pages", file.pages(), 5);
    check!("old page kept", read(&file, 1, 8), Ok(vec![5; 8]));
    check!("new page", read(&file, 4, 8), Ok(vec![0; 8]));
    check!("by nothing", file.grow(0), Ok(5));
    check!("too far", file.grow(u64::MAX), Err(PageError::OutOfRange));
    check!("too long for a file", file.grow(1 << 61), Err(PageError::OutOfRange));
    check!("unchanged", (file.pages(), std::fs::metadata(folder.file("a")).unwrap().len()), (5, 40));
}

#[test]
fn two_handles_see_the_same_file() {
    let folder = Folder::new();
    let a = PageFile::create(&folder.file("a"), 8, 2).unwrap();
    let b = PageFile::open(&folder.file("a"), 8).unwrap();
    a.write(0, &[3; 8]).unwrap();
    check!("through the other", read(&b, 0, 8), Ok(vec![3; 8]));
}

#[test]
fn random_use_matches_a_byte_vector() {
    let mut rng = anneal_prelude::Rng::new(111_101);
    for round in 0..20 {
        let folder = Folder::new();
        let size = 1 + rng.below(40);
        let mut model = vec![0u8; size * rng.below(6)];
        let mut file = PageFile::create(&folder.file("a"), size, (model.len() / size) as u64).unwrap();
        for step in 0..60 {
            let pages = (model.len() / size) as u64;
            let n = rng.below(pages as usize + 2) as u64;
            let what = format!("round {round} step {step} (page size {size}, page {n} of {pages})");
            match rng.below(8) {
                0 => {
                    let extra = rng.below(3);
                    check!(what, file.grow(extra as u64), Ok(pages));
                    model.resize(model.len() + extra * size, 0);
                }
                1 => {
                    drop(file);
                    file = PageFile::open(&folder.file("a"), size).unwrap();
                }
                2..=4 => {
                    let page: Vec<u8> = rng.vec(size, 0, 255);
                    let got = file.write(n, &page);
                    check!(what, got.is_ok(), n < pages);
                    if n < pages {
                        model[n as usize * size..][..size].copy_from_slice(&page);
                    }
                }
                _ => {
                    let want = model.get(n as usize * size..(n as usize + 1) * size).map(<[u8]>::to_vec);
                    check!(what, read(&file, n, size).ok(), want);
                }
            }
        }
        check!(format!("round {round}: the file"), std::fs::read(folder.file("a")).unwrap(), model);
    }
}
