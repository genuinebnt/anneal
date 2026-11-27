pub const SECTOR: usize = 64;
pub type Sector = [u8; SECTOR];
/// Sectors `0..SPARE` are yours. The data is in the sectors from `SPARE` on.
pub const SPARE: usize = 24;
/// The most sectors one update may change.
pub const MAX_CHANGES: usize = 8;

/// The power failed. Every later call fails too, until the machine starts again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerLost;

/// A disk of 64-byte sectors. What is written may stay in the disk's own memory until `sync`.
pub trait Disk {
    fn sectors(&self) -> usize;
    fn read(&mut self, n: usize) -> Result<Sector, PowerLost>;
    fn write(&mut self, n: usize, data: &Sector) -> Result<(), PowerLost>;
    /// Everything written before this call survives a power cut after it.
    fn sync(&mut self) -> Result<(), PowerLost>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    TooMany,
    BadSector,
    PowerLost,
}

impl From<PowerLost> for UpdateError {
    fn from(_: PowerLost) -> UpdateError {
        UpdateError::PowerLost
    }
}

/// Changes all the listed sectors, or after a power cut and `recover`, none of them.
pub fn update<D: Disk>(disk: &mut D, changes: &[(usize, Sector)]) -> Result<(), UpdateError> {
    todo!()
}

/// Run after every restart, before anything else touches the disk.
pub fn recover<D: Disk>(disk: &mut D) -> Result<(), PowerLost> {
    todo!()
}
