use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
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
        fs::create_dir_all(&dir)?;
        Ok(DirDisk { dir })
    }
}

impl Disk for DirDisk {
    fn create(&mut self, name: &str) -> io::Result<()> {
        File::create(self.dir.join(name))?;
        Ok(())
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().append(true).open(self.dir.join(name))?;
        file.write_all(bytes)
    }
    fn sync_file(&mut self, name: &str) -> io::Result<()> {
        File::open(self.dir.join(name))?.sync_all()
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        fs::rename(self.dir.join(from), self.dir.join(to))
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        fs::remove_file(self.dir.join(name))
    }
    fn sync_dir(&mut self) -> io::Result<()> {
        // on Unix a directory can be opened and synced like a file
        File::open(&self.dir)?.sync_all()
    }
    fn list(&mut self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                if let Ok(name) = entry.file_name().into_string() {
                    names.push(name);
                }
            }
        }
        Ok(names)
    }
    fn read(&mut self, name: &str) -> io::Result<Vec<u8>> {
        fs::read(self.dir.join(name))
    }
}

/// The number in `state-<number>`, if the whole name has that form.
fn generation(name: &str) -> Option<u64> {
    let digits = name.strip_prefix("state-")?;
    if digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn newest<D: Disk>(disk: &mut D) -> io::Result<Option<u64>> {
    Ok(disk.list()?.iter().filter_map(|n| generation(n)).max())
}

/// Stores a new version of the state and returns its number.
pub fn save<D: Disk>(disk: &mut D, bytes: &[u8]) -> io::Result<u64> {
    let next = newest(disk)?.unwrap_or(0) + 1;
    let name = format!("state-{next}");
    let tmp = format!("{name}.tmp");
    disk.create(&tmp)?;
    disk.write(&tmp, bytes)?;
    disk.sync_file(&tmp)?; // the bytes must be safe before the name can be
    disk.rename(&tmp, &name)?;
    disk.sync_dir()?; // only now is the new version sure to be found
    // the save has succeeded: tidying up may fail without taking that back
    if let Ok(names) = disk.list() {
        for old in names {
            if generation(&old).is_some_and(|g| g < next) {
                let _ = disk.remove(&old);
            }
        }
    }
    Ok(next)
}

/// What a program that has just started should use: the newest version and its number.
pub fn recover<D: Disk>(disk: &mut D) -> io::Result<Option<(u64, Vec<u8>)>> {
    for name in disk.list()? {
        if name.strip_suffix(".tmp").and_then(generation).is_some() {
            disk.remove(&name)?;
        }
    }
    let Some(found) = newest(disk)? else {
        return Ok(None);
    };
    let bytes = disk.read(&format!("state-{found}"))?;
    Ok(Some((found, bytes)))
}
