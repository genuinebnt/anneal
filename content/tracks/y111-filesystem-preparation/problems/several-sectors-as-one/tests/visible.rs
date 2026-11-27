use solution::*;
use std::collections::BTreeSet;

/// Sectors on the test disks: the spare area and 16 of data.
const TOTAL: usize = SPARE + 16;

#[derive(Clone)]
struct Fake {
    safe: Vec<Sector>,
    now: Vec<Sector>,
    pending: Vec<(usize, Sector)>,
    /// Writes and syncs that will still succeed.
    left: usize,
    dead: bool,
    written: BTreeSet<usize>,
}

impl Fake {
    fn from(sectors: Vec<Sector>) -> Fake {
        Fake { safe: sectors.clone(), now: sectors, pending: Vec::new(), left: usize::MAX, dead: false, written: BTreeSet::new() }
    }

    /// Data sector `i` holds the byte `i + 1`; the spare area is zero.
    fn fresh() -> Fake {
        Fake::from((0..TOTAL).map(|i| [if i < SPARE { 0 } else { (i - SPARE + 1) as u8 }; SECTOR]).collect())
    }

    fn data(&self) -> Vec<Sector> {
        self.now[SPARE..].to_vec()
    }

    /// Disks that may be found after the power fails now.
    fn after_cut(&self, seed: u64) -> Vec<Vec<Sector>> {
        let n = self.pending.len();
        let apply = |disk: &mut Vec<Sector>, i: usize, quarters: u32| {
            let (target, data) = &self.pending[i];
            for q in 0..4 {
                if quarters >> q & 1 == 1 {
                    disk[*target][q * 16..(q + 1) * 16].copy_from_slice(&data[q * 16..(q + 1) * 16]);
                }
            }
        };
        let mut found = Vec::new();
        // every choice of which writes landed whole (a sample of them if there are very many)
        let mut rng = anneal_prelude::Rng::new(seed);
        for i in 0..(1usize << n.min(9)) {
            let mask = if n <= 9 { i as u64 } else { rng.next_u64() };
            let mut disk = self.safe.clone();
            (0..n).filter(|w| mask >> (w % 64) & 1 == 1).for_each(|w| apply(&mut disk, w, 15));
            found.push(disk);
        }
        // one write torn in each way, the others all there or all missing
        for torn in 0..n {
            for quarters in 1..15 {
                for others in [0, 15] {
                    let mut disk = self.safe.clone();
                    (0..n).for_each(|w| apply(&mut disk, w, if w == torn { quarters } else { others }));
                    found.push(disk);
                }
            }
        }
        // anything at all: any order, any tears
        for _ in 0..30 {
            let mut disk = self.safe.clone();
            let mut order: Vec<usize> = (0..n).collect();
            rng.shuffle(&mut order);
            for w in order {
                apply(&mut disk, w, [0, 15, 15, rng.below(16) as u32][rng.below(4)]);
            }
            found.push(disk);
        }
        found
    }
}

impl Disk for Fake {
    fn sectors(&self) -> usize {
        self.now.len()
    }
    fn read(&mut self, n: usize) -> Result<Sector, PowerLost> {
        if self.dead {
            return Err(PowerLost);
        }
        Ok(self.now[n])
    }
    fn write(&mut self, n: usize, data: &Sector) -> Result<(), PowerLost> {
        self.step()?;
        assert!(self.pending.len() < 500, "hundreds of writes with no sync");
        self.written.insert(n);
        self.now[n] = *data;
        self.pending.push((n, *data));
        Ok(())
    }
    fn sync(&mut self) -> Result<(), PowerLost> {
        self.step()?;
        self.safe = self.now.clone();
        self.pending.clear();
        Ok(())
    }
}

impl Fake {
    /// One more write or sync, unless the power fails first.
    fn step(&mut self) -> Result<(), PowerLost> {
        if self.dead || self.left == 0 {
            self.dead = true;
            return Err(PowerLost);
        }
        self.left -= 1;
        Ok(())
    }
}

fn changes(targets: &[usize], byte: u8) -> Vec<(usize, Sector)> {
    targets
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut sector = [byte.wrapping_add(i as u8); SECTOR];
            sector[63] = i as u8;
            (*t, sector)
        })
        .collect()
}

fn applied(data: &[Sector], changes: &[(usize, Sector)]) -> Vec<Sector> {
    let mut out = data.to_vec();
    for (t, sector) in changes {
        out[t - SPARE] = *sector;
    }
    out
}

/// Restarts from a disk found after a cut, recovers, and returns the disk.
fn restart(sectors: Vec<Sector>) -> Fake {
    let mut disk = Fake::from(sectors);
    assert_eq!(recover(&mut disk), Ok(()), "recover on a working disk");
    disk
}

#[test]
fn an_update_changes_the_listed_sectors() {
    let mut disk = Fake::fresh();
    let list = changes(&[SPARE + 1, SPARE + 5], 100);
    let want = applied(&disk.data(), &list);
    check!("update", update(&mut disk, &list), Ok(()));
    check!("data", disk.data() == want, true);
}

#[test]
fn what_returned_ok_is_on_the_disk() {
    let mut disk = Fake::fresh();
    let list = changes(&[SPARE, SPARE + 2, SPARE + 4], 50);
    let want = applied(&disk.data(), &list);
    update(&mut disk, &list).unwrap();
    for found in disk.after_cut(1) {
        check!("after any cut", restart(found).data() == want, true);
    }
}

#[test]
fn bad_requests_write_nothing() {
    let mut disk = Fake::fresh();
    check!("nine", update(&mut disk, &changes(&(SPARE..SPARE + 9).collect::<Vec<_>>(), 1)), Err(UpdateError::TooMany));
    for bad in [vec![0], vec![SPARE - 1], vec![TOTAL], vec![usize::MAX], vec![SPARE, SPARE + 1, SPARE]] {
        check!(format!("{bad:?}"), update(&mut disk, &changes(&bad, 1)), Err(UpdateError::BadSector));
    }
    check!("nothing written", disk.written.len(), 0);
}

#[test]
fn recover_on_a_fresh_disk_leaves_the_data() {
    let mut disk = Fake::fresh();
    let before = disk.data();
    check!("recover", recover(&mut disk), Ok(()));
    check!("data", disk.data() == before, true);
}

#[test]
fn a_clean_cut_between_two_writes() {
    let list = changes(&[SPARE + 3, SPARE + 9], 70);
    let old = Fake::fresh().data();
    let new = applied(&old, &list);
    for k in 0..40 {
        let mut disk = Fake::fresh();
        disk.left = k;
        let done = update(&mut disk, &list).is_ok();
        let got = restart(disk.now.clone()).data();
        check!(format!("cut after {k} writes and syncs: old or new"), got == old || got == new, true);
        check!(format!("cut after {k} writes and syncs: done means new"), !done || got == new, true);
    }
}
