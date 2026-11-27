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

// The spare area: sector 0 is the record that commits an update, sectors 1 to 8 hold copies of the new content.
// Record: magic [0..8], count [8..12], targets [12..44], checksum [44..52].
const MAGIC: &[u8; 8] = b"COMMITv1";

fn checksum(record: &Sector, copies: &[Sector]) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325u64;
    let _ = copies;
    for byte in record[..44].iter() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn field(record: &Sector, at: usize) -> usize {
    u32::from_le_bytes([record[at], record[at + 1], record[at + 2], record[at + 3]]) as usize
}

/// Changes all the listed sectors, or after a power cut and `recover`, none of them.
pub fn update<D: Disk>(disk: &mut D, changes: &[(usize, Sector)]) -> Result<(), UpdateError> {
    if changes.len() > MAX_CHANGES {
        return Err(UpdateError::TooMany);
    }
    for (i, (target, _)) in changes.iter().enumerate() {
        if *target < SPARE
            || *target >= disk.sectors()
            || changes[..i].iter().any(|(t, _)| t == target)
        {
            return Err(UpdateError::BadSector);
        }
    }
    if changes.is_empty() {
        return Ok(());
    }
    // 1. the new content goes aside
    let mut record = [0u8; SECTOR];
    record[..8].copy_from_slice(MAGIC);
    record[8..12].copy_from_slice(&(changes.len() as u32).to_le_bytes());
    let mut copies = Vec::new();
    for (i, (target, data)) in changes.iter().enumerate() {
        record[12 + 4 * i..16 + 4 * i].copy_from_slice(&(*target as u32).to_le_bytes());
        disk.write(1 + i, data)?;
        copies.push(*data);
    }
    // 2. one sector decides
    let sum = checksum(&record, &copies);
    record[44..52].copy_from_slice(&sum.to_le_bytes());
    disk.write(0, &record)?;
    disk.sync()?;
    // 3. the new content goes home, and only then is the record dropped
    for (target, data) in changes {
        disk.write(*target, data)?;
    }
    disk.sync()?;
    disk.write(0, &[0; SECTOR])?;
    disk.sync()?;
    Ok(())
}

/// Run after every restart, before anything else touches the disk.
pub fn recover<D: Disk>(disk: &mut D) -> Result<(), PowerLost> {
    let record = disk.read(0)?;
    if record.iter().all(|b| *b == 0) {
        return Ok(());
    }
    let count = field(&record, 8);
    if &record[..8] == MAGIC && count <= MAX_CHANGES {
        let targets: Vec<usize> = (0..count).map(|i| field(&record, 12 + 4 * i)).collect();
        let mut copies = Vec::new();
        for i in 0..count {
            copies.push(disk.read(1 + i)?);
        }
        let stored = record[44..52] == checksum(&record, &copies).to_le_bytes();
        if stored && targets.iter().all(|t| (SPARE..disk.sectors()).contains(t)) {
            for (target, copy) in targets.iter().zip(&copies) {
                disk.write(*target, copy)?;
            }
            disk.sync()?;
        }
    }
    disk.write(0, &[0; SECTOR])?;
    disk.sync()
}
