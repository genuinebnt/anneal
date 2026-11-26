use std::io;
use std::path::PathBuf;

/// The files of one directory.
pub trait Disk {
    /// A new, empty file under this name, replacing any that had it.
    fn create(&mut self, name: &str) -> io::Result<()>;
    /// Adds bytes at the end of a file that exists. On error, any prefix of them may have been added.
    fn write(&mut self, name: &str, bytes: &[u8]) -> io::Result<()>;
    /// Makes the file's bytes safe from a power cut (not its name).
    fn sync_file(&mut self, name: &str) -> io::Result<()>;
    /// Gives `from`'s file the name `to`, replacing any file that had it.
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;
    fn remove(&mut self, name: &str) -> io::Result<()>;
    /// Makes the directory's names, as they are now, safe from a power cut (not any bytes).
    fn sync_dir(&mut self) -> io::Result<()>;
    /// The names of the files in the directory, in any order.
    fn list(&mut self) -> io::Result<Vec<String>>;
    fn read(&mut self, name: &str) -> io::Result<Vec<u8>>;
}

/// A real directory.
pub struct DirDisk {
    dir: PathBuf,
}

impl DirDisk {
    /// Uses `dir`, creating it and its parents if they are missing.
    pub fn open(dir: PathBuf) -> io::Result<DirDisk> {
        todo!()
    }
}

impl Disk for DirDisk {
    fn create(&mut self, name: &str) -> io::Result<()> {
        todo!()
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        todo!()
    }
    fn sync_file(&mut self, name: &str) -> io::Result<()> {
        todo!()
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        todo!()
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        todo!()
    }
    fn sync_dir(&mut self) -> io::Result<()> {
        todo!()
    }
    fn list(&mut self) -> io::Result<Vec<String>> {
        todo!()
    }
    fn read(&mut self, name: &str) -> io::Result<Vec<u8>> {
        todo!()
    }
}

/// Stores a new version of the state and returns its number.
pub fn save<D: Disk>(disk: &mut D, bytes: &[u8]) -> io::Result<u64> {
    todo!()
}

/// What a program that has just started should use: the newest version and its number.
pub fn recover<D: Disk>(disk: &mut D) -> io::Result<Option<(u64, Vec<u8>)>> {
    todo!()
}
