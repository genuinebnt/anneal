//! RECIPE 244 (storage): disk-head scheduling (FCFS/SSTF/SCAN/C-SCAN/LOOK/C-LOOK), RAID 0/1/5, and checksum scrubbing.
//! LEVEL: intermediate
//! Use in: understanding storage stacks, OS courses, designing redundancy and repair in your own store.   Run: cargo run --release --example disk_scheduling_raid
#![allow(dead_code)]
//!
//! WHAT: (a) a spinning-disk model where service cost = seek distance (cylinders) + 1 transfer tick, with six
//! head-scheduling policies run on seeded random workloads; (b) RAID levels 0, 1 and 5 over in-memory block devices
//! with per-disk I/O counters, degraded reads, rebuild, and the RAID 5 write hole; (c) a checksummed mirror whose
//! scrub detects silent corruption and heals from the good copy.
//!
//! SCHEDULING: FCFS is fair but seeks wildly. SSTF (shortest seek first) minimises each hop but can STARVE far
//! requests while a stream of nearby ones keeps arriving. SCAN (elevator) sweeps to the disk edge and back; C-SCAN
//! sweeps one way and jumps back (more uniform waits); LOOK/C-LOOK turn around at the last request instead of the edge.
//! SSDs/NVMe have no moving head, so seek-aware ordering mostly disappears; what matters there is queue depth
//! (parallelism across flash channels; NVMe has many deep queues), write amplification (flash is erased in large blocks
//! and rewritten out of place, so small random writes cost more internal writes than the host sent) and TRIM.
//!
//! RAID: RAID 0 stripes (speed, no redundancy). RAID 1 mirrors (survives N-1 losses, capacity of one disk). RAID 5 keeps
//! one XOR parity block per stripe, rotated across disks so no disk becomes the parity bottleneck. Because
//! p = d0^d1^...^dk, any one lost block = XOR of all the others. A small write needs read-modify-write:
//! new_parity = old_parity ^ old_data ^ new_data, i.e. 2 reads + 2 writes (the "write penalty" of 4 I/Os), whereas a
//! full-stripe write computes parity from the new data alone with no reads. THE WRITE HOLE: data and parity are two
//! separate writes; a crash between them leaves parity stale, and a later disk loss silently reconstructs WRONG data
//! for blocks that were never being written. Fixes: a journal / write-intent log, battery-backed cache, or
//! copy-on-write with variable stripe width (ZFS RAID-Z).
//!
//! SCRUB: checksums stored apart from the data they cover (ZFS keeps them in the parent block pointer, Btrfs checksums
//! data and metadata) let you tell WHICH mirror copy is right; a plain mirror can only notice that copies differ.
//!
//! COMMON USES: OS schedulers (Linux has mq-deadline/BFQ/none; NVMe usually uses none), storage arrays, home NAS,
//! cloud block stores, periodic `zpool scrub`-style integrity checks, erasure coding in object stores (RAID 5 generalised).
//! Read: https://pages.cs.wisc.edu/~remzi/OSTEP/file-disks.pdf https://pages.cs.wisc.edu/~remzi/OSTEP/file-raid.pdf

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ================================================================ (a) head scheduling
const CYLS: i64 = 200;
#[derive(Clone, Copy, Debug)]
struct Req {
    id: usize,
    cyl: i64,
    arrival: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Policy {
    Fcfs,
    Sstf,
    Scan,
    CScan,
    Look,
    CLook,
}
const POLICIES: [Policy; 6] = [
    Policy::Fcfs,
    Policy::Sstf,
    Policy::Scan,
    Policy::CScan,
    Policy::Look,
    Policy::CLook,
];

struct Head {
    pos: i64,
    up: bool,
}
impl Policy {
    /// Choose the next pending request; returns (index into `pending`, head movement in cylinders).
    fn pick(self, h: &mut Head, pending: &[Req]) -> (usize, i64) {
        let best = |f: &dyn Fn(&Req) -> bool, key: &dyn Fn(&Req) -> i64| {
            pending
                .iter()
                .enumerate()
                .filter(|(_, r)| f(r))
                .min_by_key(|(_, r)| (key(r), r.id))
                .map(|(i, _)| i)
        };
        match self {
            Policy::Fcfs => {
                let i = best(&|_| true, &|r| r.arrival as i64).unwrap();
                (i, (pending[i].cyl - h.pos).abs())
            }
            Policy::Sstf => {
                let i = best(&|_| true, &|r| (r.cyl - h.pos).abs()).unwrap();
                (i, (pending[i].cyl - h.pos).abs())
            }
            Policy::Scan | Policy::Look => {
                let ahead = if h.up {
                    best(&|r| r.cyl >= h.pos, &|r| r.cyl - h.pos)
                } else {
                    best(&|r| r.cyl <= h.pos, &|r| h.pos - r.cyl)
                };
                if let Some(i) = ahead {
                    return (i, (pending[i].cyl - h.pos).abs());
                }
                // nothing ahead: SCAN first runs to the physical edge, LOOK turns around where it is
                let edge = if h.up { CYLS - 1 } else { 0 };
                let (to_edge, from) = if self == Policy::Scan {
                    ((edge - h.pos).abs(), edge)
                } else {
                    (0, h.pos)
                };
                h.up = !h.up;
                let i = if h.up {
                    best(&|r| r.cyl >= from, &|r| r.cyl - from)
                } else {
                    best(&|r| r.cyl <= from, &|r| from - r.cyl)
                }
                .unwrap();
                (i, to_edge + (pending[i].cyl - from).abs())
            }
            Policy::CScan | Policy::CLook => {
                // one-directional sweep: when nothing is ahead, return to the start (edge for C-SCAN, lowest request for C-LOOK)
                if let Some(i) = best(&|r| r.cyl >= h.pos, &|r| r.cyl - h.pos) {
                    return (i, (pending[i].cyl - h.pos).abs());
                }
                let i = best(&|_| true, &|r| r.cyl).unwrap();
                let cost = if self == Policy::CScan {
                    (CYLS - 1 - h.pos) + (CYLS - 1) + pending[i].cyl
                } else {
                    (h.pos - pending[i].cyl).abs()
                };
                (i, cost)
            }
        }
    }
}
struct SchedResult {
    movement: i64,
    mean_wait: f64,
    max_wait: u64,
    position_of: Vec<usize>,
}
/// Event simulation: each service costs `movement + 1` ticks; requests become visible at their arrival tick.
fn schedule(policy: Policy, start: i64, reqs: &[Req]) -> SchedResult {
    let mut todo: Vec<Req> = reqs.to_vec();
    todo.sort_by_key(|r| (r.arrival, r.id));
    let mut todo = std::collections::VecDeque::from(todo);
    let (mut pending, mut now, mut head) = (
        Vec::new(),
        0u64,
        Head {
            pos: start,
            up: true,
        },
    );
    let (mut movement, mut waits, mut position_of) = (0i64, Vec::new(), vec![0; reqs.len()]);
    let mut served = 0;
    while served < reqs.len() {
        while todo.front().is_some_and(|r| r.arrival <= now) {
            pending.push(todo.pop_front().unwrap());
        }
        if pending.is_empty() {
            now = todo.front().unwrap().arrival;
            continue;
        }
        let (i, cost) = policy.pick(&mut head, &pending);
        let r = pending.swap_remove(i);
        movement += cost;
        now += cost as u64 + 1;
        head.pos = r.cyl;
        waits.push(now - r.arrival);
        position_of[r.id] = served;
        served += 1;
    }
    SchedResult {
        movement,
        mean_wait: waits.iter().sum::<u64>() as f64 / waits.len() as f64,
        max_wait: *waits.iter().max().unwrap(),
        position_of,
    }
}

fn part_a() {
    println!("== (a) head scheduling: {CYLS} cylinders, cost = seek distance + 1 ==");
    let mut rng = Rng(0x5eed_0001);
    let trials = 200;
    let mut sum = [0i64; 6];
    let mut worst = [0u64; 6];
    for _ in 0..trials {
        let reqs: Vec<Req> = (0..40)
            .map(|id| Req {
                id,
                cyl: rng.below(CYLS as u64) as i64,
                arrival: 0,
            })
            .collect();
        let start = rng.below(CYLS as u64) as i64;
        for (k, p) in POLICIES.iter().enumerate() {
            let r = schedule(*p, start, &reqs);
            sum[k] += r.movement;
            worst[k] = worst[k].max(r.max_wait);
        }
    }
    println!(
        "{:8} {:>22} {:>16}",
        "policy", "avg head movement", "worst max wait"
    );
    for (k, p) in POLICIES.iter().enumerate() {
        println!(
            "{:8} {:>22.0} {:>16}",
            format!("{p:?}"),
            sum[k] as f64 / trials as f64,
            worst[k]
        );
    }
    let (fcfs, sstf, scan, look) = (sum[0], sum[1], sum[2], sum[4]);
    assert!(
        scan < fcfs && look < fcfs && sstf < fcfs,
        "seek-aware policies beat FCFS on random queues"
    );
    assert!(
        look <= scan,
        "LOOK never travels to the edge, so it moves no more than SCAN"
    );

    // every policy serves each request exactly once and the head never teleports for free
    let reqs: Vec<Req> = (0..30)
        .map(|id| Req {
            id,
            cyl: rng.below(CYLS as u64) as i64,
            arrival: 0,
        })
        .collect();
    for p in POLICIES {
        let mut pos = schedule(p, 100, &reqs).position_of;
        pos.sort();
        assert_eq!(pos, (0..30).collect::<Vec<_>>());
    }

    // SSTF starvation: request 0 sits at the far edge while a dense stream of near-head requests keeps arriving
    let mut reqs = vec![Req {
        id: 0,
        cyl: CYLS - 1,
        arrival: 0,
    }];
    for i in 1..=300 {
        reqs.push(Req {
            id: i,
            cyl: 98 + (i as i64 % 5),
            arrival: i as u64 / 2,
        });
    } // 2 arrivals per tick > service rate
    let (sstf, fcfs, scan) = (
        schedule(Policy::Sstf, 100, &reqs),
        schedule(Policy::Fcfs, 100, &reqs),
        schedule(Policy::Scan, 100, &reqs),
    );
    println!(
        "starvation workload (far request + 300 nearby arrivals): far request served at position SSTF {} / SCAN {} / FCFS {} of 301",
        sstf.position_of[0], scan.position_of[0], fcfs.position_of[0]
    );
    assert!(
        sstf.position_of[0] > 250,
        "SSTF keeps picking the nearby stream"
    );
    println!(
        "  max wait (ticks): SSTF {} SCAN {} FCFS {} (aggregate waits hide starvation: look at the far request's service position)",
        sstf.max_wait, scan.max_wait, fcfs.max_wait
    );
    assert!(scan.position_of[0] < 120 && fcfs.position_of[0] <= 1);
    assert_eq!(sstf.position_of[0], 300, "served dead last");
    println!(
        "SSD/NVMe: no seek term at all, so reordering by cylinder is pointless; queue depth and write amplification dominate."
    );
}

// ================================================================ (b) RAID
const BS: usize = 16;
#[derive(Debug, PartialEq)]
enum IoErr {
    Failed(usize),
    Range,
    Unrecoverable,
}
#[derive(Clone)]
struct Disk {
    blocks: Vec<[u8; BS]>,
    failed: bool,
    reads: usize,
    writes: usize,
}
impl Disk {
    fn new(n: usize) -> Self {
        Disk {
            blocks: vec![[0; BS]; n],
            failed: false,
            reads: 0,
            writes: 0,
        }
    }
    fn read(&mut self, b: usize, id: usize) -> Result<[u8; BS], IoErr> {
        if self.failed {
            return Err(IoErr::Failed(id));
        }
        self.reads += 1;
        Ok(self.blocks[b])
    }
    fn write(&mut self, b: usize, d: [u8; BS], id: usize) -> Result<(), IoErr> {
        if self.failed {
            return Err(IoErr::Failed(id));
        }
        self.writes += 1;
        self.blocks[b] = d;
        Ok(())
    }
}
fn xor(a: [u8; BS], b: [u8; BS]) -> [u8; BS] {
    let mut o = a;
    for i in 0..BS {
        o[i] ^= b[i];
    }
    o
}
fn ios(disks: &[Disk]) -> (usize, usize) {
    (
        disks.iter().map(|d| d.reads).sum(),
        disks.iter().map(|d| d.writes).sum(),
    )
}

trait Volume {
    fn name(&self) -> &'static str;
    fn capacity(&self) -> usize;
    fn read(&mut self, lba: usize) -> Result<[u8; BS], IoErr>;
    fn write(&mut self, lba: usize, d: [u8; BS]) -> Result<(), IoErr>;
    fn disks(&mut self) -> &mut Vec<Disk>;
}
struct Raid0 {
    d: Vec<Disk>,
}
impl Volume for Raid0 {
    fn name(&self) -> &'static str {
        "RAID0"
    }
    fn capacity(&self) -> usize {
        self.d.len() * self.d[0].blocks.len()
    }
    fn read(&mut self, lba: usize) -> Result<[u8; BS], IoErr> {
        let n = self.d.len();
        self.d[lba % n].read(lba / n, lba % n)
    }
    fn write(&mut self, lba: usize, v: [u8; BS]) -> Result<(), IoErr> {
        let n = self.d.len();
        self.d[lba % n].write(lba / n, v, lba % n)
    }
    fn disks(&mut self) -> &mut Vec<Disk> {
        &mut self.d
    }
}
struct Raid1 {
    d: Vec<Disk>,
}
impl Volume for Raid1 {
    fn name(&self) -> &'static str {
        "RAID1"
    }
    fn capacity(&self) -> usize {
        self.d[0].blocks.len()
    }
    fn read(&mut self, lba: usize) -> Result<[u8; BS], IoErr> {
        for (i, d) in self.d.iter_mut().enumerate() {
            if let Ok(b) = d.read(lba, i) {
                return Ok(b);
            }
        }
        Err(IoErr::Unrecoverable)
    }
    fn write(&mut self, lba: usize, v: [u8; BS]) -> Result<(), IoErr> {
        let mut ok = false;
        for (i, d) in self.d.iter_mut().enumerate() {
            ok |= d.write(lba, v, i).is_ok();
        }
        if ok {
            Ok(())
        } else {
            Err(IoErr::Unrecoverable)
        }
    }
    fn disks(&mut self) -> &mut Vec<Disk> {
        &mut self.d
    }
}
struct Raid5 {
    d: Vec<Disk>,
}
impl Raid5 {
    fn n(&self) -> usize {
        self.d.len()
    }
    fn parity_disk(&self, row: usize) -> usize {
        self.n() - 1 - row % self.n()
    } // rotates every stripe
    /// lba -> (row, data disk, parity disk)
    fn locate(&self, lba: usize) -> (usize, usize, usize) {
        let (row, idx) = (lba / (self.n() - 1), lba % (self.n() - 1));
        let p = self.parity_disk(row);
        (row, if idx < p { idx } else { idx + 1 }, p)
    }
    fn xor_others(&mut self, row: usize, skip: &[usize]) -> Result<[u8; BS], IoErr> {
        let mut acc = [0; BS];
        for i in 0..self.n() {
            if !skip.contains(&i) {
                acc = xor(acc, self.d[i].read(row, i)?);
            }
        }
        Ok(acc)
    }
    /// Crash injection: data reaches the disk, the parity update never happens (the write hole).
    fn write_data_only(&mut self, lba: usize, v: [u8; BS]) {
        let (row, d, _) = self.locate(lba);
        self.d[d].write(row, v, d).unwrap();
    }
    fn full_stripe_write(&mut self, row: usize, data: &[[u8; BS]]) {
        assert_eq!(data.len(), self.n() - 1);
        let p = self.parity_disk(row);
        let mut parity = [0; BS];
        let mut it = data.iter();
        for i in 0..self.n() {
            if i == p {
                continue;
            }
            let b = *it.next().unwrap();
            parity = xor(parity, b);
            self.d[i].write(row, b, i).unwrap();
        }
        self.d[p].write(row, parity, p).unwrap(); // n writes, 0 reads
    }
    fn rebuild(&mut self, i: usize) {
        let rows = self.d[0].blocks.len();
        self.d[i] = Disk::new(rows); // hot-swapped replacement
        for row in 0..rows {
            let b = self.xor_others(row, &[i]).unwrap();
            self.d[i].write(row, b, i).unwrap();
        }
    }
    fn parity_consistent(&mut self) -> bool {
        (0..self.d[0].blocks.len()).all(|row| self.xor_others(row, &[]).unwrap() == [0; BS])
    }
}
impl Volume for Raid5 {
    fn name(&self) -> &'static str {
        "RAID5"
    }
    fn capacity(&self) -> usize {
        (self.n() - 1) * self.d[0].blocks.len()
    }
    fn read(&mut self, lba: usize) -> Result<[u8; BS], IoErr> {
        let (row, d, _) = self.locate(lba);
        match self.d[d].read(row, d) {
            Ok(b) => Ok(b),
            Err(_) => self.xor_others(row, &[d]).map_err(|_| IoErr::Unrecoverable),
        } // degraded read
    }
    fn write(&mut self, lba: usize, v: [u8; BS]) -> Result<(), IoErr> {
        let (row, d, p) = self.locate(lba);
        match (self.d[d].failed, self.d[p].failed) {
            (false, false) => {
                // read-modify-write: 2 reads + 2 writes
                let (od, op) = (self.d[d].read(row, d)?, self.d[p].read(row, p)?);
                let np = xor(xor(op, od), v);
                self.d[d].write(row, v, d)?;
                self.d[p].write(row, np, p)
            }
            (true, false) => {
                // data disk lost: fold the new data into parity so a later rebuild yields it
                let others = self.xor_others(row, &[d, p])?;
                self.d[p].write(row, xor(others, v), p)
            }
            (false, true) => self.d[d].write(row, v, d), // parity disk lost: just write the data
            (true, true) => Err(IoErr::Unrecoverable),
        }
    }
    fn disks(&mut self) -> &mut Vec<Disk> {
        &mut self.d
    }
}

fn rand_block(rng: &mut Rng) -> [u8; BS] {
    let mut b = [0; BS];
    for x in &mut b {
        *x = rng.next() as u8;
    }
    b
}
fn exercise<V: Volume>(v: &mut V, rng: &mut Rng) -> Vec<[u8; BS]> {
    let mut model = vec![[0u8; BS]; v.capacity()];
    for _ in 0..v.capacity() * 4 {
        let lba = rng.below(v.capacity() as u64) as usize;
        let b = rand_block(rng);
        v.write(lba, b).unwrap();
        model[lba] = b;
    }
    for (lba, m) in model.iter().enumerate() {
        assert_eq!(v.read(lba).unwrap(), *m, "{} lba {lba}", v.name());
    }
    model
}

fn part_b() {
    println!("\n== (b) RAID over in-memory disks ({BS}-byte blocks) ==");
    let mut rng = Rng(0xa1d5_0002);
    let rows = 32;
    let mut r0 = Raid0 {
        d: vec![Disk::new(rows); 4],
    };
    let mut r1 = Raid1 {
        d: vec![Disk::new(rows); 2],
    };
    let mut r5 = Raid5 {
        d: vec![Disk::new(rows); 5],
    };
    exercise(&mut r0, &mut rng);
    exercise(&mut r1, &mut rng);
    let model = exercise(&mut r5, &mut rng);
    assert!(
        r5.parity_consistent(),
        "XOR of every row (data + parity) is zero"
    );
    // RAID0 has no redundancy; RAID1 survives either disk
    r0.d[1].failed = true;
    assert_eq!(r0.read(1), Err(IoErr::Failed(1)));
    assert!(r0.read(0).is_ok());
    let m1 = exercise(&mut r1, &mut rng);
    for dead in 0..2 {
        r1.d[dead].failed = true;
        for (l, m) in m1.iter().enumerate() {
            assert_eq!(r1.read(l).unwrap(), *m);
        }
        r1.d[dead].failed = false;
    }

    // RAID5: every single-disk failure: degraded reads, degraded writes, rebuild is byte-for-byte identical
    for dead in 0..5 {
        let mut a = Raid5 { d: r5.d.clone() };
        let snapshot = a.d[dead].blocks.clone();
        a.d[dead].failed = true;
        for (l, m) in model.iter().enumerate() {
            assert_eq!(a.read(l).unwrap(), *m, "degraded read, disk {dead} down");
        }
        let mut model2 = model.clone();
        for _ in 0..200 {
            let l = rng.below(a.capacity() as u64) as usize;
            let b = rand_block(&mut rng);
            a.write(l, b).unwrap();
            model2[l] = b;
        }
        for (l, m) in model2.iter().enumerate() {
            assert_eq!(
                a.read(l).unwrap(),
                *m,
                "degraded write then read, disk {dead} down"
            );
        }
        a.d[(dead + 1) % 5].failed = true;
        assert!(
            model.iter().enumerate().any(|(l, _)| a.read(l).is_err()),
            "two failures lose data"
        );
        a.d[(dead + 1) % 5].failed = false;
        a.rebuild(dead);
        assert!(a.parity_consistent());
        for (l, m) in model2.iter().enumerate() {
            assert_eq!(a.read(l).unwrap(), *m);
        }
        if model2 == model {
            assert_eq!(a.d[dead].blocks, snapshot);
        }
    }
    // rebuilt contents equal what the disk held (no writes happened in between)
    let mut a = Raid5 { d: r5.d.clone() };
    let want = a.d[2].blocks.clone();
    a.d[2].failed = true;
    a.rebuild(2);
    assert_eq!(
        a.d[2].blocks, want,
        "rebuilt disk is byte-for-byte identical"
    );
    println!(
        "RAID5 x5: all 5 single-disk failures survive degraded read/write; rebuild is byte-for-byte exact"
    );

    // the write penalty, counted at the disks
    let before = ios(&r0.d);
    r0.d[1].failed = false;
    r0.write(5, [1; BS]).unwrap();
    let (r0r, r0w) = (ios(&r0.d).0 - before.0, ios(&r0.d).1 - before.1);
    let before = ios(&r1.d);
    r1.write(5, [1; BS]).unwrap();
    let (r1r, r1w) = (ios(&r1.d).0 - before.0, ios(&r1.d).1 - before.1);
    let before = ios(&r5.d);
    r5.write(5, [1; BS]).unwrap();
    let (r5r, r5w) = (ios(&r5.d).0 - before.0, ios(&r5.d).1 - before.1);
    println!(
        "one small write -> disk I/Os: RAID0 {r0r}r+{r0w}w, RAID1 {r1r}r+{r1w}w, RAID5 {r5r}r+{r5w}w (new_parity = old_parity ^ old_data ^ new_data)"
    );
    assert_eq!((r0r, r0w, r1r, r1w, r5r, r5w), (0, 1, 0, 2, 2, 2));
    let before = ios(&r5.d);
    let stripe: Vec<[u8; BS]> = (0..4).map(|_| rand_block(&mut rng)).collect();
    r5.full_stripe_write(3, &stripe);
    let after = ios(&r5.d);
    assert_eq!((after.0 - before.0, after.1 - before.1), (0, 5));
    assert!(r5.parity_consistent());
    println!("full-stripe write on RAID5: 0 reads + 5 writes for 4 data blocks (no penalty)");

    // write hole: crash after data, before parity
    let mut h = Raid5 { d: r5.d.clone() };
    let (row, d_new, p) = h.locate(8);
    h.write_data_only(8, [0xEE; BS]); // power cut before the parity write
    assert!(!h.parity_consistent());
    let victim_lba = (0..h.capacity())
        .find(|&l| {
            let (r, d, _) = h.locate(l);
            r == row && d != d_new
        })
        .unwrap();
    let (_, victim_disk, _) = h.locate(victim_lba);
    let truth = h.read(victim_lba).unwrap();
    h.d[victim_disk].failed = true; // an unrelated disk now dies
    let recovered = h.read(victim_lba).unwrap();
    assert_ne!(
        recovered, truth,
        "reconstruction used stale parity: silent corruption of an innocent block"
    );
    println!(
        "write hole: block on disk {victim_disk} (never written) reconstructed wrongly after a crash between data (disk {d_new}) and parity (disk {p})"
    );

    println!("\n level  disks  capacity  tolerates  small-write I/Os  read throughput");
    println!(" RAID0  N      N*S       0          1                 N x");
    println!(
        " RAID1  N      S         N-1        N writes          up to N x (read from any copy)"
    );
    println!(
        " RAID5  N>=3   (N-1)*S   1          2r+2w = 4         ~(N-1) x; degraded reads touch all survivors"
    );
    println!(" RAID6  N>=4   (N-2)*S   2          6 I/Os (3r+3w)    like RAID5, two parities");
}

// ================================================================ (c) checksummed mirror + scrub
fn fnv1a(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325, |h, &x| {
        (h ^ x as u64).wrapping_mul(0x100_0000_01b3)
    })
}
struct CsMirror {
    copies: [Vec<[u8; BS]>; 2],
    sums: Vec<u64>,
} // sums live "elsewhere" (like a parent pointer)
#[derive(Default, Debug)]
struct ScrubReport {
    checked: usize,
    healed: usize,
    lost: usize,
}
impl CsMirror {
    fn new(n: usize) -> Self {
        CsMirror {
            copies: [vec![[0; BS]; n], vec![[0; BS]; n]],
            sums: vec![fnv1a(&[0; BS]); n],
        }
    }
    fn write(&mut self, i: usize, d: [u8; BS]) {
        self.copies[0][i] = d;
        self.copies[1][i] = d;
        self.sums[i] = fnv1a(&d);
    }
    /// Verified read: try each copy, return the first whose checksum matches, repairing the other (self-healing read).
    fn read(&mut self, i: usize) -> Result<[u8; BS], IoErr> {
        for c in 0..2 {
            if fnv1a(&self.copies[c][i]) == self.sums[i] {
                self.copies[1 - c][i] = self.copies[c][i];
                return Ok(self.copies[c][i]);
            }
        }
        Err(IoErr::Unrecoverable)
    }
    fn scrub(&mut self) -> ScrubReport {
        let mut r = ScrubReport::default();
        for i in 0..self.sums.len() {
            r.checked += 1;
            let ok = [0, 1].map(|c| fnv1a(&self.copies[c][i]) == self.sums[i]);
            match ok {
                [true, true] => {}
                [true, false] => {
                    self.copies[1][i] = self.copies[0][i];
                    r.healed += 1;
                }
                [false, true] => {
                    self.copies[0][i] = self.copies[1][i];
                    r.healed += 1;
                }
                [false, false] => r.lost += 1,
            }
        }
        r
    }
}
fn part_c() {
    println!("\n== (c) checksum scrub over a mirror ==");
    let mut rng = Rng(0xc0ff_ee03);
    let n = 256;
    let mut m = CsMirror::new(n);
    let model: Vec<[u8; BS]> = (0..n).map(|_| rand_block(&mut rng)).collect();
    for (i, b) in model.iter().enumerate() {
        m.write(i, *b);
    }
    // silent corruption: a bit flips on ONE copy per victim block; the device still reports success
    let mut victims = std::collections::BTreeSet::new();
    while victims.len() < 40 {
        victims.insert(rng.below(n as u64) as usize);
    }
    for &v in &victims {
        m.copies[rng.below(2) as usize][v][rng.below(BS as u64) as usize] ^= 1 << rng.below(8);
    }
    // a plain mirror reading copy 0 would return garbage here without noticing
    let plain_wrong = victims
        .iter()
        .filter(|&&v| m.copies[0][v] != model[v])
        .count();
    // verified read repairs on the fly for the first few victims
    for &v in victims.iter().take(5) {
        assert_eq!(m.read(v).unwrap(), model[v]);
    }
    let report = m.scrub();
    println!(
        "40 corrupted blocks ({plain_wrong} on copy 0, which a plain mirror would hand to the application); scrub: {report:?}"
    );
    assert_eq!(report.checked, n);
    assert_eq!(report.healed, 35);
    assert_eq!(report.lost, 0);
    assert!(
        m.copies[0] == model && m.copies[1] == model,
        "both copies identical to the original after healing"
    );
    assert_eq!(m.scrub().healed, 0, "a second scrub finds nothing");
    // both copies damaged: detected (not silently returned) but unrecoverable without a third copy/parity
    m.copies[0][7][0] ^= 1;
    m.copies[1][7][3] ^= 4;
    assert_eq!(m.read(7), Err(IoErr::Unrecoverable));
    assert_eq!(m.scrub().lost, 1);
    println!(
        "both copies bad -> reported as lost, never returned as good data (needs a third copy or parity)"
    );
}

fn main() {
    part_a();
    part_b();
    part_c();
    println!("\nOK");
}
