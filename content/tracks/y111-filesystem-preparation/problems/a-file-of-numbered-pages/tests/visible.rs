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
fn a_new_file_is_all_zeros() {
    let folder = Folder::new();
    let file = PageFile::create(&folder.file("a"), 16, 4).unwrap();
    check!("pages", file.pages(), 4);
    check!("length on disk", std::fs::metadata(folder.file("a")).unwrap().len(), 64);
    check!("page 3", read(&file, 3, 16), Ok(vec![0; 16]));
}

#[test]
fn a_page_goes_to_its_own_place() {
    let folder = Folder::new();
    let file = PageFile::create(&folder.file("a"), 8, 3).unwrap();
    check!("write page 1", file.write(1, &[7; 8]), Ok(()));
    check!("sync", file.sync(), Ok(()));
    let mut want = vec![0; 24];
    want[8..16].fill(7);
    check!("the bytes", std::fs::read(folder.file("a")).unwrap(), want);
}

#[test]
fn pages_are_still_there_after_reopening() {
    let folder = Folder::new();
    let file = PageFile::create(&folder.file("a"), 512, 10).unwrap();
    file.write(9, &[9; 512]).unwrap();
    drop(file);
    let file = PageFile::open(&folder.file("a"), 512).unwrap();
    check!("pages", file.pages(), 10);
    check!("page 9", read(&file, 9, 512), Ok(vec![9; 512]));
}

#[test]
fn past_the_end_is_an_error_not_a_longer_file() {
    let folder = Folder::new();
    let file = PageFile::create(&folder.file("a"), 8, 2).unwrap();
    check!("write page 2", file.write(2, &[1; 8]), Err(PageError::OutOfRange));
    check!("read page 2", read(&file, 2, 8), Err(PageError::OutOfRange));
    check!("length", std::fs::metadata(folder.file("a")).unwrap().len(), 16);
}

#[test]
fn creating_never_destroys() {
    let folder = Folder::new();
    std::fs::write(folder.file("keep"), b"precious").unwrap();
    let got = PageFile::create(&folder.file("keep"), 8, 1).map(|_| ());
    check!("create over a file", got, Err(PageError::Io(ErrorKind::AlreadyExists)));
    check!("untouched", std::fs::read(folder.file("keep")).unwrap(), b"precious".to_vec());
}
