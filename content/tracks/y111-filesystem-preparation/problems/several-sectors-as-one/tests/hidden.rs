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

/// Runs the update with the power failing after `k` writes and syncs, for every `k`, and looks at every disk that could be left.
fn every_cut(start: &Fake, list: &[(usize, Sector)]) {
    let old = start.data();
    let new = applied(&old, list);
    for k in 0.. {
        assert!(k < 200, "an update should not need hundreds of calls");
        let mut disk = start.clone();
        disk.written.clear();
        disk.left = k;
        let result = update(&mut disk, list);
        let done = result.is_ok();
        if !done {
            check!(format!("cut after {k} writes and syncs: the error"), result, Err(UpdateError::PowerLost));
        }
        for (i, found) in disk.after_cut(k as u64 + 7).into_iter().enumerate() {
            let after = restart(found);
            let got = after.data();
            check!(format!("{} changes, cut after {k} writes and syncs, disk {i}: old or new", list.len()), got == old || got == new, true);
            if done {
                check!(format!("returned Ok, disk {i}: new"), got == new, true);
            }
            let allowed: BTreeSet<usize> = list.iter().map(|(t, _)| *t).collect();
            check!("recover writes only the spare area and the update's sectors", after.written.iter().all(|s| *s < SPARE || allowed.contains(s)), true);
        }
        check!("update writes only the spare area and the listed sectors", disk.written.iter().all(|s| *s < SPARE || list.iter().any(|(t, _)| t == s)), true);
        if done {
            return;
        }
    }
}

#[test]
fn every_cut_of_one_change() {
    every_cut(&Fake::fresh(), &changes(&[SPARE + 7], 90));
}

#[test]
fn every_cut_of_two_changes() {
    every_cut(&Fake::fresh(), &changes(&[SPARE + 15, SPARE], 110));
}

#[test]
fn every_cut_of_eight_changes() {
    every_cut(&Fake::fresh(), &changes(&[31, 24, 39, 25, 33, 28, 36, 30], 120));
}

#[test]
fn every_cut_of_an_update_after_another() {
    // what the first update left in the spare area is still there
    let mut disk = Fake::fresh();
    update(&mut disk, &changes(&(SPARE..SPARE + 8).collect::<Vec<_>>(), 10)).unwrap();
    every_cut(&disk, &changes(&[SPARE + 8, SPARE + 9, SPARE + 10], 140));
    every_cut(&disk, &changes(&[SPARE, SPARE + 1, SPARE + 2], 10));
}

#[test]
fn a_cut_during_recovery() {
    let list = changes(&[SPARE + 2, SPARE + 6, SPARE + 11, SPARE + 12], 150);
    let old = Fake::fresh().data();
    let new = applied(&old, &list);
    for k in 0..30 {
        let mut disk = Fake::fresh();
        disk.left = k;
        let done = update(&mut disk, &list).is_ok();
        // start from a few of the disks the first cut can leave, and cut the recovery everywhere
        for (i, found) in disk.after_cut(900 + k as u64).into_iter().step_by(11).enumerate() {
            let first = restart(found.clone()).data();
            for j in 0..16 {
                let mut again = Fake::from(found.clone());
                again.left = j;
                let _ = recover(&mut again);
                for second in again.after_cut(j as u64).into_iter().step_by(5) {
                    let got = restart(second).data();
                    check!(format!("update cut at {k}, disk {i}, recovery cut at {j}"), got == old || got == new, true);
                    // once a recovery has decided on the new content, a later one may not go back
                    check!("done stays done", !done || got == new, true);
                }
            }
            check!("old or new", first == old || first == new, true);
        }
    }
}

#[test]
fn recovering_twice_is_the_same() {
    let list = changes(&[SPARE + 4, SPARE + 5], 160);
    for k in 0..20 {
        let mut disk = Fake::fresh();
        disk.left = k;
        let _ = update(&mut disk, &list);
        let mut once = restart(disk.now.clone());
        let data = once.data();
        check!("second recover", recover(&mut once), Ok(()));
        check!(format!("cut after {k}: same data"), once.data() == data, true);
    }
}

#[test]
fn rubbish_in_the_spare_area_is_ignored() {
    let mut rng = anneal_prelude::Rng::new(111_601);
    for round in 0..300 {
        let mut disk = Fake::fresh();
        for sector in 0..SPARE {
            let fill = rng.below(3);
            for byte in disk.now[sector].iter_mut() {
                *byte = match fill {
                    0 => 0xFF,
                    1 => rng.below(4) as u8,
                    _ => rng.below(256) as u8,
                };
            }
        }
        let before = disk.data();
        check!(format!("round {round}: recover"), recover(&mut disk), Ok(()));
        check!(format!("round {round}: data"), disk.data() == before, true);
        // and the disk still works
        let list = changes(&[SPARE + 1], 3);
        check!(format!("round {round}: update"), update(&mut disk, &list), Ok(()));
        check!(format!("round {round}: updated"), disk.data() == applied(&before, &list), true);
    }
}

#[test]
fn an_empty_update_is_fine() {
    let mut disk = Fake::fresh();
    let before = disk.data();
    check!("update", update(&mut disk, &[]), Ok(()));
    check!("data", disk.data() == before, true);
}

#[test]
fn a_long_history_of_updates_and_cuts() {
    let mut rng = anneal_prelude::Rng::new(111_602);
    for round in 0..120 {
        let mut disk = Fake::fresh();
        let mut data = disk.data();
        for step in 0..12 {
            let mut targets: Vec<usize> = (SPARE..TOTAL).collect();
            rng.shuffle(&mut targets);
            targets.truncate(1 + rng.below(8));
            // few different values, so that old content turns up again
            let list: Vec<(usize, Sector)> = targets.iter().map(|t| (*t, [rng.below(3) as u8; SECTOR])).collect();
            let new = applied(&data, &list);
            disk.left = if rng.bool() { rng.below(30) } else { usize::MAX };
            let done = update(&mut disk, &list).is_ok();
            if done {
                check!(format!("round {round} step {step}"), disk.data() == new, true);
                disk.left = usize::MAX;
                if rng.bool() {
                    data = new;
                    continue;
                }
            }
            let mut found = disk.after_cut(rng.next_u64());
            let pick = rng.below(found.len());
            disk = restart(found.swap_remove(pick));
            let got = disk.data();
            check!(format!("round {round} step {step}: old or new"), got == new || (!done && got == data), true);
            data = got;
        }
    }
}
