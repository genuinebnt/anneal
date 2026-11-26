//! RECIPE 240 (os): virtual memory: a 4-level page-table + TLB + demand-paging simulator, and the real thing via mmap/mprotect.
//! LEVEL: advanced
//! Use in: understanding page faults, allocators, guard pages, memory-mapped IO, why RSS != virtual size.   Run: cargo run --release --example virtual_memory_sim
#![allow(dead_code)]
//!
//! WHAT: Part A simulates an x86-64-style MMU: 48-bit virtual addresses = 9+9+9+9 index bits + 12 offset bits, a radix
//! page table whose leaf entries carry present/writable/accessed/dirty bits, a small set-associative LRU TLB, page
//! faults, demand paging from a simulated disk and four page-replacement policies (FIFO, LRU, CLOCK, OPT) behind one
//! trait. Part B uses the real kernel (Unix only): anonymous `mmap`, `mprotect`, lazy commit (RSS grows only when
//! pages are touched), `madvise`, guard pages and controlled SIGSEGV/SIGBUS deaths observed from a forked child.
//!
//! WHY: virtual memory gives isolation, lazy allocation and the illusion of more memory than RAM. The costs are
//! TLB misses (a 4-level walk = up to 4 memory reads) and page faults (microseconds to milliseconds).
//!
//! HOW: replacement policies see the reference string; OPT (Belady's MIN) looks at the future, so it is the lower
//! bound. LRU and OPT are "stack algorithms": more frames never means more faults. FIFO is not: Belady's anomaly.
//! In the real VM, a fault on a PROT_NONE page or an unmapped page delivers SIGSEGV (macOS may deliver SIGBUS).
//! After `munmap` the address range is unmapped (or, worse, reused by another allocation): touching it is
//! undefined behaviour in Rust and a crash at best, silent corruption at worst. macOS differences: the page size is
//! 16 KiB on Apple Silicon, `MADV_DONTNEED` is advisory (does not zero), and `ru_maxrss` is in bytes (KiB on Linux).
//!
//! COMMON USES: allocators (jemalloc/glibc malloc use mmap + madvise to return memory), language runtimes with guard
//! pages for stack overflow detection, JVM/Go heap reservation (reserve with PROT_NONE, commit with mprotect),
//! databases and memory-mapped files, copy-on-write fork, OS course exercises on paging policies.
//! Read: https://pages.cs.wisc.edu/~remzi/OSTEP/vm-tlbs.pdf https://pages.cs.wisc.edu/~remzi/OSTEP/vm-beyondphys-policy.pdf https://man7.org/linux/man-pages/man2/mmap.2.html https://man7.org/linux/man-pages/man2/mprotect.2.html
use std::collections::{HashMap, VecDeque};

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

// ---------------------------------------------------------------- Part A: page replacement
trait Replacer {
    fn name(&self) -> &'static str;
    fn on_hit(&mut self, _page: u64, _t: usize) {}
    fn on_load(&mut self, page: u64, t: usize);
    fn victim(&mut self, resident: &[u64], t: usize) -> u64;
    fn on_evict(&mut self, _page: u64) {}
}

#[derive(Default)]
struct Fifo(VecDeque<u64>);
impl Replacer for Fifo {
    fn name(&self) -> &'static str {
        "FIFO"
    }
    fn on_load(&mut self, p: u64, _t: usize) {
        self.0.push_back(p)
    }
    fn victim(&mut self, _r: &[u64], _t: usize) -> u64 {
        *self.0.front().unwrap()
    }
    fn on_evict(&mut self, p: u64) {
        self.0.retain(|&x| x != p)
    }
}

#[derive(Default)]
struct Lru(HashMap<u64, usize>);
impl Replacer for Lru {
    fn name(&self) -> &'static str {
        "LRU"
    }
    fn on_hit(&mut self, p: u64, t: usize) {
        self.0.insert(p, t);
    }
    fn on_load(&mut self, p: u64, t: usize) {
        self.0.insert(p, t);
    }
    fn victim(&mut self, r: &[u64], _t: usize) -> u64 {
        *r.iter().min_by_key(|p| self.0[p]).unwrap()
    }
    fn on_evict(&mut self, p: u64) {
        self.0.remove(&p);
    }
}

/// Second-chance / CLOCK: a ring of (page, referenced) and a hand that clears bits until it finds a 0.
#[derive(Default)]
struct Clock {
    ring: Vec<(u64, bool)>,
    hand: usize,
    free_slot: Option<usize>,
}
impl Replacer for Clock {
    fn name(&self) -> &'static str {
        "CLOCK"
    }
    fn on_hit(&mut self, p: u64, _t: usize) {
        if let Some(e) = self.ring.iter_mut().find(|e| e.0 == p) {
            e.1 = true;
        }
    }
    fn on_load(&mut self, p: u64, _t: usize) {
        match self.free_slot.take() {
            Some(i) => {
                self.ring[i] = (p, true);
                self.hand = (i + 1) % self.ring.len();
            }
            None => self.ring.push((p, true)),
        }
    }
    fn victim(&mut self, _r: &[u64], _t: usize) -> u64 {
        loop {
            let h = self.hand % self.ring.len();
            if self.ring[h].1 {
                self.ring[h].1 = false;
                self.hand = (h + 1) % self.ring.len();
            } else {
                return self.ring[h].0;
            }
        }
    }
    fn on_evict(&mut self, p: u64) {
        self.free_slot = self.ring.iter().position(|e| e.0 == p);
    }
}

/// Belady's OPT/MIN: evict the page whose next use is farthest in the future. Needs the whole trace, so it is only
/// a yardstick, never implementable in a real kernel.
struct Opt(Vec<u64>);
impl Replacer for Opt {
    fn name(&self) -> &'static str {
        "OPT"
    }
    fn on_load(&mut self, _p: u64, _t: usize) {}
    fn victim(&mut self, r: &[u64], t: usize) -> u64 {
        let next = |p: u64| {
            self.0[t + 1..]
                .iter()
                .position(|&x| x == p)
                .unwrap_or(usize::MAX)
        };
        *r.iter().max_by_key(|&&p| next(p)).unwrap()
    }
}

fn simulate(policy: &mut dyn Replacer, refs: &[u64], frames: usize) -> usize {
    let mut resident: Vec<u64> = Vec::new();
    let mut faults = 0;
    for (t, &page) in refs.iter().enumerate() {
        if resident.contains(&page) {
            policy.on_hit(page, t);
            continue;
        }
        faults += 1;
        if resident.len() == frames {
            let v = policy.victim(&resident, t);
            policy.on_evict(v);
            resident.retain(|&x| x != v);
        }
        resident.push(page);
        policy.on_load(page, t);
    }
    faults
}
fn make(name: &str, refs: &[u64]) -> Box<dyn Replacer> {
    match name {
        "FIFO" => Box::new(Fifo::default()),
        "LRU" => Box::new(Lru::default()),
        "CLOCK" => Box::new(Clock::default()),
        _ => Box::new(Opt(refs.to_vec())),
    }
}
const POLICIES: [&str; 4] = ["FIFO", "LRU", "CLOCK", "OPT"];

// ---------------------------------------------------------------- Part A: MMU
const P: u64 = 1; // present
const W: u64 = 2; // writable
const A: u64 = 4; // accessed
const D: u64 = 8; // dirty
const PAGE: usize = 4096;

fn split(va: u64) -> ([usize; 4], usize) {
    let idx = |shift: u32| ((va >> shift) & 0x1ff) as usize;
    ([idx(39), idx(30), idx(21), idx(12)], (va & 0xfff) as usize)
}
fn join(idx: [usize; 4], off: usize) -> u64 {
    ((idx[0] as u64) << 39)
        | ((idx[1] as u64) << 30)
        | ((idx[2] as u64) << 21)
        | ((idx[3] as u64) << 12)
        | off as u64
}

#[derive(Clone, Copy)]
struct TlbEntry {
    vpn: u64,
    frame: usize,
    writable: bool,
    dirty: bool,
    stamp: u64,
}
struct Tlb {
    sets: Vec<Vec<TlbEntry>>,
    ways: usize,
    clock: u64,
    hits: u64,
    misses: u64,
}
impl Tlb {
    fn new(sets: usize, ways: usize) -> Self {
        Tlb {
            sets: vec![Vec::new(); sets],
            ways,
            clock: 0,
            hits: 0,
            misses: 0,
        }
    }
    fn lookup(&mut self, vpn: u64) -> Option<TlbEntry> {
        self.clock += 1;
        let n = self.sets.len();
        let clock = self.clock;
        match self.sets[vpn as usize % n]
            .iter_mut()
            .find(|e| e.vpn == vpn)
        {
            Some(e) => {
                e.stamp = clock;
                self.hits += 1;
                Some(*e)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }
    fn insert(&mut self, mut e: TlbEntry) {
        self.clock += 1;
        e.stamp = self.clock;
        let n = self.sets.len();
        let set = &mut self.sets[e.vpn as usize % n];
        set.retain(|x| x.vpn != e.vpn);
        if set.len() == self.ways {
            let i = (0..set.len()).min_by_key(|&i| set[i].stamp).unwrap();
            set.swap_remove(i);
        }
        set.push(e);
    }
    fn invalidate(&mut self, vpn: u64) {
        let n = self.sets.len();
        self.sets[vpn as usize % n].retain(|e| e.vpn != vpn);
    }
    fn set_dirty(&mut self, vpn: u64) {
        let n = self.sets.len();
        if let Some(e) = self.sets[vpn as usize % n]
            .iter_mut()
            .find(|e| e.vpn == vpn)
        {
            e.dirty = true;
        }
    }
    fn hit_rate(&self) -> f64 {
        self.hits as f64 / (self.hits + self.misses).max(1) as f64
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Fault {
    Segv,
    Protection,
    NonCanonical,
}
#[derive(Default, Debug)]
struct Stats {
    walks: u64,
    faults: u64,
    disk_reads: u64,
    disk_writes: u64,
    zero_fills: u64,
    evictions: u64,
    prot: u64,
}
struct Vma {
    start: u64,
    end: u64,
    writable: bool,
} // page numbers, [start, end)

struct Mmu {
    nodes: Vec<Box<[u64; 512]>>, // arena of page-table pages; node 0 is the root (CR3)
    tlb: Tlb,
    vmas: Vec<Vma>,
    frames: Vec<Vec<u8>>,
    free: Vec<usize>,
    resident: Vec<u64>,
    disk: HashMap<u64, Vec<u8>>,
    policy: Box<dyn Replacer>,
    t: usize,
    stats: Stats,
}
impl Mmu {
    fn new(nframes: usize, policy: Box<dyn Replacer>) -> Self {
        Mmu {
            nodes: vec![Box::new([0; 512])],
            tlb: Tlb::new(16, 4),
            vmas: vec![],
            frames: vec![vec![0; PAGE]; nframes],
            free: (0..nframes).rev().collect(),
            resident: vec![],
            disk: HashMap::new(),
            policy,
            t: 0,
            stats: Stats::default(),
        }
    }
    fn map(&mut self, start_va: u64, pages: u64, writable: bool) {
        self.vmas.push(Vma {
            start: start_va >> 12,
            end: (start_va >> 12) + pages,
            writable,
        });
    }
    /// Walk the 3 upper levels; returns (node, index) of the leaf PTE. With `alloc` missing tables are created.
    fn walk(&mut self, vpn: u64, alloc: bool) -> Option<(usize, usize)> {
        let (idx, _) = split(vpn << 12);
        let mut node = 0;
        for &i in &idx[..3] {
            let e = self.nodes[node][i];
            if e & P == 0 {
                if !alloc {
                    return None;
                }
                self.nodes.push(Box::new([0; 512]));
                self.nodes[node][i] = P | (((self.nodes.len() - 1) as u64) << 12);
                node = self.nodes.len() - 1;
            } else {
                node = (e >> 12) as usize;
            }
        }
        Some((node, idx[3]))
    }
    fn access(&mut self, va: u64, write: bool) -> Result<usize, Fault> {
        if va >> 48 != 0 {
            return Err(Fault::NonCanonical);
        }
        let t = self.t;
        self.t += 1;
        let (vpn, off) = (va >> 12, (va & 0xfff) as usize);
        if let Some(e) = self.tlb.lookup(vpn) {
            if write && !e.writable {
                self.stats.prot += 1;
                return Err(Fault::Protection);
            }
            if write && !e.dirty {
                let (n, i) = self.walk(vpn, false).unwrap();
                self.nodes[n][i] |= D;
                self.tlb.set_dirty(vpn);
            }
            self.policy.on_hit(vpn, t);
            return Ok(e.frame * PAGE + off);
        }
        let mut faulted = false;
        loop {
            self.stats.walks += 1;
            if let Some((n, i)) = self.walk(vpn, false) {
                let pte = self.nodes[n][i];
                if pte & P != 0 {
                    if write && pte & W == 0 {
                        self.stats.prot += 1;
                        return Err(Fault::Protection);
                    }
                    let new = pte | A | if write { D } else { 0 };
                    self.nodes[n][i] = new;
                    let frame = (new >> 12) as usize;
                    self.tlb.insert(TlbEntry {
                        vpn,
                        frame,
                        writable: new & W != 0,
                        dirty: new & D != 0,
                        stamp: 0,
                    });
                    if !faulted {
                        self.policy.on_hit(vpn, t);
                    }
                    return Ok(frame * PAGE + off);
                }
            }
            self.page_fault(vpn, write, t)?;
            faulted = true;
        }
    }
    fn page_fault(&mut self, vpn: u64, write: bool, t: usize) -> Result<(), Fault> {
        let vma = self
            .vmas
            .iter()
            .find(|v| v.start <= vpn && vpn < v.end)
            .ok_or(Fault::Segv)?;
        let writable = vma.writable;
        if write && !writable {
            self.stats.prot += 1;
            return Err(Fault::Protection);
        }
        self.stats.faults += 1;
        let frame = match self.free.pop() {
            Some(f) => f,
            None => {
                let v = self.policy.victim(&self.resident, t);
                self.policy.on_evict(v);
                self.resident.retain(|&x| x != v);
                let (n, i) = self.walk(v, false).unwrap();
                let pte = self.nodes[n][i];
                let f = (pte >> 12) as usize;
                if pte & D != 0 {
                    self.disk.insert(v, self.frames[f].clone());
                    self.stats.disk_writes += 1;
                }
                self.nodes[n][i] = 0; // not present any more
                self.tlb.invalidate(v); // otherwise the TLB would keep translating to a reused frame
                self.stats.evictions += 1;
                f
            }
        };
        match self.disk.get(&vpn) {
            Some(d) => {
                self.frames[frame].copy_from_slice(d);
                self.stats.disk_reads += 1;
            }
            None => {
                self.frames[frame].fill(0);
                self.stats.zero_fills += 1;
            }
        }
        let (n, i) = self.walk(vpn, true).unwrap();
        self.nodes[n][i] = P | if writable { W } else { 0 } | ((frame as u64) << 12);
        self.policy.on_load(vpn, t);
        self.resident.push(vpn);
        Ok(())
    }
    fn read(&mut self, va: u64) -> Result<u8, Fault> {
        let pa = self.access(va, false)?;
        Ok(self.frames[pa / PAGE][pa % PAGE])
    }
    fn write(&mut self, va: u64, v: u8) -> Result<(), Fault> {
        let pa = self.access(va, true)?;
        self.frames[pa / PAGE][pa % PAGE] = v;
        Ok(())
    }
}

fn part_a() {
    println!("== Part A: simulated MMU ==");
    let mut rng = Rng(0x9e3779b97f4a7c15);
    // 1. address split round trip
    for _ in 0..20_000 {
        let va = rng.next() & ((1 << 48) - 1);
        let (idx, off) = split(va);
        assert!(idx.iter().all(|&i| i < 512) && off < 4096);
        assert_eq!(join(idx, off), va);
    }
    println!("20000 random 48-bit addresses split into 9+9+9+9+12 bits and re-joined");

    // 2. Belady's anomaly
    let classic = [1, 2, 3, 4, 1, 2, 5, 1, 2, 3, 4, 5];
    let (f3, f4) = (
        simulate(&mut Fifo::default(), &classic, 3),
        simulate(&mut Fifo::default(), &classic, 4),
    );
    println!(
        "FIFO classic string: 3 frames -> {f3} faults, 4 frames -> {f4} faults (Belady's anomaly)"
    );
    assert_eq!((f3, f4), (9, 10));
    assert_eq!(simulate(&mut Opt(classic.to_vec()), &classic, 3), 7);

    // 3. random strings: OPT <= all; LRU/OPT monotone in frames
    let mut totals = [0usize; 4];
    let mut fifo_anomalies = 0;
    for _ in 0..60 {
        let len = 150;
        let refs: Vec<u64> = (0..len)
            .map(|_| {
                if rng.below(10) < 7 {
                    rng.below(6)
                } else {
                    rng.below(14)
                }
            })
            .collect();
        let mut prev = [usize::MAX; 4];
        for frames in 1..=10 {
            let r: Vec<usize> = POLICIES
                .iter()
                .map(|n| simulate(&mut *make(n, &refs), &refs, frames))
                .collect();
            assert!(
                r[3] <= r[0] && r[3] <= r[1] && r[3] <= r[2],
                "OPT must be a lower bound"
            );
            for k in [1, 3] {
                assert!(
                    r[k] <= prev[k],
                    "{} is a stack algorithm: no anomaly",
                    POLICIES[k]
                );
            }
            if r[0] > prev[0] && prev[0] != usize::MAX {
                fifo_anomalies += 1;
            }
            if frames == 4 {
                for k in 0..4 {
                    totals[k] += r[k];
                }
            }
            prev = [r[0], r[1], r[2], r[3]];
        }
    }
    println!(
        "faults at 4 frames summed over 60 random strings: FIFO {} LRU {} CLOCK {} OPT {}",
        totals[0], totals[1], totals[2], totals[3]
    );
    println!("FIFO anomalies seen in random strings (more frames, more faults): {fifo_anomalies}");

    // 4. MMU: sparse address space, demand paging, data survives eviction
    let mut mmu = Mmu::new(8, Box::new(Lru::default()));
    let (code, heap, stack) = (0x40_0000u64, 0x1000_0000u64, 0x7fff_ffff_0000u64);
    mmu.map(code, 4, false);
    mmu.map(heap, 64, true);
    mmu.map(stack, 4, true);
    let mut model: HashMap<u64, u8> = HashMap::new();
    for _ in 0..30_000 {
        // 80% of accesses hit 6 hot pages, the rest wander over the whole heap
        let page = if rng.below(10) < 8 {
            rng.below(6)
        } else {
            rng.below(64)
        };
        let va = heap + page * 4096 + rng.below(4096);
        if rng.below(3) == 0 {
            let v = rng.next() as u8;
            mmu.write(va, v).unwrap();
            model.insert(va, v);
        } else {
            assert_eq!(mmu.read(va).unwrap(), model.get(&va).copied().unwrap_or(0));
        }
    }
    mmu.write(stack + 8, 1).unwrap();
    mmu.read(code + 5).unwrap();
    let mut kv: Vec<_> = model.iter().collect();
    kv.sort();
    for (&va, &v) in kv {
        assert_eq!(
            mmu.read(va).unwrap(),
            v,
            "value must survive eviction to disk and back"
        );
    }
    println!(
        "page-table pages allocated for 3 far-apart regions: {} (a flat table would need 2^36 entries)",
        mmu.nodes.len()
    );
    println!(
        "TLB hit rate {:.1}% ; {:?}",
        mmu.tlb.hit_rate() * 100.0,
        mmu.stats
    );
    assert!(mmu.stats.evictions > 0 && mmu.stats.disk_writes > 0 && mmu.stats.disk_reads > 0);

    // 5. translation consistency: physical address maps back to the PTE frame
    let pa = mmu.access(heap + 0x123, false).unwrap();
    assert_eq!(pa % PAGE, 0x123);
    let (n, i) = mmu.walk((heap + 0x123) >> 12, false).unwrap();
    assert_eq!((mmu.nodes[n][i] >> 12) as usize, pa / PAGE);
    assert!(mmu.nodes[n][i] & (P | A) == (P | A));

    // 6. protection and segmentation faults
    let mut m2 = Mmu::new(4, Box::new(Fifo::default()));
    m2.map(0x1000, 2, false);
    assert_eq!(m2.read(0x1000), Ok(0));
    assert_eq!(m2.write(0x1000, 1), Err(Fault::Protection));
    assert_eq!(m2.write(0x1800, 1), Err(Fault::Protection)); // still read-only, now via TLB hit
    assert_eq!(m2.read(0x9000), Err(Fault::Segv));
    assert_eq!(m2.read(1 << 48), Err(Fault::NonCanonical));
    println!(
        "write to read-only page -> Protection, unmapped -> Segv, non-canonical -> NonCanonical"
    );

    // 7. MMU fault counts equal the trace-driven simulator (reads of one byte per page)
    let refs: Vec<u64> = (0..400)
        .map(|_| {
            if rng.below(10) < 7 {
                rng.below(5)
            } else {
                rng.below(20)
            }
        })
        .collect();
    for name in POLICIES {
        let mut m = Mmu::new(6, make(name, &refs));
        m.map(0, 64, true);
        for &p in &refs {
            m.read(p * 4096).unwrap();
        }
        let sim = simulate(&mut *make(name, &refs), &refs, 6);
        println!("{name:5} faults MMU={} sim={sim}", m.stats.faults);
        assert_eq!(m.stats.faults as usize, sim);
    }
}

// ---------------------------------------------------------------- Part B: the real thing
#[cfg(unix)]
mod real {
    use std::ptr;

    pub fn page_size() -> usize {
        // SAFETY: sysconf with a valid name has no memory-safety requirements.
        unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
    }
    /// Current resident set size in bytes, if we know how to read it here.
    pub fn rss() -> Option<usize> {
        #[cfg(target_os = "linux")]
        {
            let s = std::fs::read_to_string("/proc/self/statm").ok()?;
            Some(s.split_whitespace().nth(1)?.parse::<usize>().ok()? * page_size())
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: proc_taskinfo is plain old data; the kernel fills at most `size` bytes of it.
            unsafe {
                let mut ti: libc::proc_taskinfo = std::mem::zeroed();
                let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
                let n = libc::proc_pidinfo(
                    libc::getpid(),
                    libc::PROC_PIDTASKINFO,
                    0,
                    &mut ti as *mut _ as *mut libc::c_void,
                    size,
                );
                if n == size {
                    Some(ti.pti_resident_size as usize)
                } else {
                    None
                }
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            None
        }
    }
    pub fn max_rss_bytes() -> usize {
        // SAFETY: rusage is POD and getrusage fills it for RUSAGE_SELF.
        unsafe {
            let mut ru: libc::rusage = std::mem::zeroed();
            libc::getrusage(libc::RUSAGE_SELF, &mut ru);
            if cfg!(target_os = "macos") {
                ru.ru_maxrss as usize
            } else {
                ru.ru_maxrss as usize * 1024
            }
        }
    }

    /// An owned anonymous mapping, unmapped on drop. Optionally flanked by PROT_NONE guard pages.
    pub struct Mapping {
        base: *mut u8,
        len: usize,
        data: *mut u8,
        data_len: usize,
    }
    impl Mapping {
        pub fn anon(len: usize, prot: libc::c_int) -> Mapping {
            // SAFETY: anonymous private mapping with no address hint; result is checked against MAP_FAILED.
            let p = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    len,
                    prot,
                    libc::MAP_PRIVATE | libc::MAP_ANON,
                    -1,
                    0,
                )
            };
            assert_ne!(
                p,
                libc::MAP_FAILED,
                "mmap failed: {}",
                std::io::Error::last_os_error()
            );
            Mapping {
                base: p.cast(),
                len,
                data: p.cast(),
                data_len: len,
            }
        }
        /// `pages` usable pages with one PROT_NONE guard page before and after (stack-overflow style).
        pub fn guarded(pages: usize) -> Mapping {
            let ps = page_size();
            let m = Mapping::anon((pages + 2) * ps, libc::PROT_NONE);
            // SAFETY: the range [base+ps, base+ps+pages*ps) lies inside our own mapping.
            let r = unsafe {
                libc::mprotect(
                    m.base.add(ps).cast(),
                    pages * ps,
                    libc::PROT_READ | libc::PROT_WRITE,
                )
            };
            assert_eq!(r, 0);
            // SAFETY: still inside the mapping. `m` is forgotten so only the returned value unmaps (once).
            let g = Mapping {
                base: m.base,
                len: m.len,
                data: unsafe { m.base.add(ps) },
                data_len: pages * ps,
            };
            std::mem::forget(m);
            g
        }
        pub fn ptr(&self) -> *mut u8 {
            self.data
        }
        pub fn len(&self) -> usize {
            self.data_len
        }
        pub fn protect(&self, off: usize, len: usize, prot: libc::c_int) -> i32 {
            assert!(off + len <= self.data_len);
            // SAFETY: range is within the mapping we own; offsets are page aligned by callers.
            unsafe { libc::mprotect(self.data.add(off).cast(), len, prot) }
        }
        pub fn forget(self) -> (*mut u8, usize) {
            let r = (self.base, self.len);
            std::mem::forget(self);
            r
        }
    }
    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: base/len describe exactly the mapping created in the constructor, unmapped once.
            unsafe { libc::munmap(self.base.cast(), self.len) };
        }
    }

    #[derive(Debug, PartialEq)]
    pub enum Outcome {
        Exited(i32),
        Signaled(i32),
    }
    /// Fork, run `f` in the child, and report how it ended. The child must only do async-signal-safe things.
    pub fn in_child(f: impl FnOnce()) -> Outcome {
        // SAFETY: the process is single-threaded here; the child only runs `f` and `_exit`s without unwinding
        // into the parent's stack frames or running destructors.
        unsafe {
            let pid = libc::fork();
            assert!(pid >= 0);
            if pid == 0 {
                let lim = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                libc::setrlimit(libc::RLIMIT_CORE, &lim); // no core dumps for our deliberate crashes
                libc::signal(libc::SIGSEGV, libc::SIG_DFL);
                libc::signal(libc::SIGBUS, libc::SIG_DFL);
                let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_ok();
                libc::_exit(if ok { 0 } else { 101 });
            }
            let mut status = 0;
            assert_eq!(libc::waitpid(pid, &mut status, 0), pid);
            if libc::WIFSIGNALED(status) {
                Outcome::Signaled(libc::WTERMSIG(status))
            } else {
                Outcome::Exited(libc::WEXITSTATUS(status))
            }
        }
    }
    extern "C" fn on_segv(_sig: libc::c_int) {
        // only async-signal-safe calls allowed here: _exit is one, println! is not
        // SAFETY: _exit is async-signal-safe and never returns.
        unsafe { libc::_exit(42) }
    }
    pub fn touch(p: *const u8) {
        // SAFETY: deliberately unsound-looking only in children that are expected to crash; callers pass a
        // pointer into a mapping that is PROT_NONE / unmapped to provoke the fault.
        unsafe { std::ptr::read_volatile(p) };
    }
    pub fn demo() {
        let ps = page_size();
        println!("\n== Part B: real virtual memory (page size {ps} bytes) ==");

        // lazy commit
        let before = rss();
        let total = 64 << 20;
        let m = Mapping::anon(total, libc::PROT_READ | libc::PROT_WRITE);
        let after_map = rss();
        let touched = 8 << 20;
        // SAFETY: we write inside our own RW mapping, one byte per page.
        unsafe {
            for off in (0..touched).step_by(ps) {
                *m.ptr().add(off) = 1;
            }
        }
        let after_touch = rss();
        println!(
            "RSS before {:?} after mmap(64MiB) {:?} after touching 8MiB {:?}",
            before, after_map, after_touch
        );
        println!(
            "ru_maxrss (high-water mark, bytes) = {}  [macOS reports bytes, Linux KiB; we normalise]",
            max_rss_bytes()
        );
        if let (Some(b), Some(a), Some(t)) = (before, after_map, after_touch) {
            assert!(a < b + (8 << 20), "mmap alone must not commit memory");
            assert!(t >= a + touched / 2, "touching pages must raise RSS");
        } else {
            println!("(no RSS probe on this OS; skipping lazy-commit asserts)");
        }

        // madvise
        // SAFETY: first page of our own mapping.
        unsafe {
            *m.ptr() = 7;
            let r = libc::madvise(m.ptr().cast(), ps, libc::MADV_DONTNEED);
            assert_eq!(r, 0);
        }
        // SAFETY: mapping still valid; MADV_DONTNEED on private anon memory leaves it mapped.
        let v = unsafe { *m.ptr() };
        println!(
            "after MADV_DONTNEED first byte = {v} (Linux: 0, pages dropped; macOS: advisory, may stay 7)"
        );
        #[cfg(target_os = "linux")]
        assert_eq!(v, 0);
        drop(m);

        // mprotect: reserve PROT_NONE, commit later
        let r = Mapping::anon(4 * ps, libc::PROT_NONE);
        assert_eq!(r.protect(0, ps, libc::PROT_READ | libc::PROT_WRITE), 0);
        // SAFETY: the first page is now RW and inside the mapping.
        unsafe {
            *r.ptr() = 99;
        }
        assert_eq!(r.protect(0, ps, libc::PROT_READ), 0);
        // SAFETY: read-only reads are fine; contents survive mprotect.
        assert_eq!(unsafe { *r.ptr() }, 99);
        println!("mprotect NONE -> RW -> R keeps contents; reserve-then-commit works");

        // guard pages
        let g = Mapping::guarded(2);
        // SAFETY: last byte of the usable region.
        unsafe {
            *g.ptr().add(g.len() - 1) = 5;
        }
        let below = g.ptr().wrapping_sub(1) as usize;
        let above = (g.ptr() as usize + g.len()) as *const u8;
        let first = in_child(|| touch(below as *const u8));
        let second = in_child(|| touch(above));
        let segv = |o: &Outcome| matches!(o, Outcome::Signaled(s) if *s == libc::SIGSEGV || *s == libc::SIGBUS);
        println!(
            "under-run into guard page: {first:?}; over-run: {second:?} (SIGSEGV={}, SIGBUS={})",
            libc::SIGSEGV,
            libc::SIGBUS
        );
        assert!(segv(&first) && segv(&second));

        // PROT_NONE and read-only writes die
        let o = in_child(|| touch(r.ptr().wrapping_add(ps)));
        println!("read of PROT_NONE page: {o:?}");
        assert!(segv(&o));
        let o = in_child(|| {
            // SAFETY: intentionally violating page protection in a doomed child to show the kernel enforcing it.
            unsafe { std::ptr::write_volatile(r.ptr(), 1) }
        });
        println!("write to PROT_READ page: {o:?}");
        assert!(segv(&o));

        // handler via sigaction: child catches SIGSEGV and exits 42
        let o = in_child(|| {
            // SAFETY: sigaction struct is zero-initialised then filled; handler is async-signal-safe.
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = on_segv as *const () as usize;
                libc::sigemptyset(&mut sa.sa_mask);
                libc::sigaction(libc::SIGSEGV, &sa, ptr::null_mut());
                libc::sigaction(libc::SIGBUS, &sa, ptr::null_mut());
            }
            touch(r.ptr().wrapping_add(ps));
        });
        println!(
            "sigaction handler in child: {o:?} (a handler can only exit/longjmp: returning re-executes the fault forever)"
        );
        assert_eq!(o, Outcome::Exited(42));

        // use after munmap
        let u = Mapping::anon(ps, libc::PROT_READ | libc::PROT_WRITE);
        let (p, len) = u.forget();
        // SAFETY: we own exactly this mapping and never touch it again in the parent.
        assert_eq!(unsafe { libc::munmap(p.cast(), len) }, 0);
        let addr = p as usize;
        let o = in_child(|| touch(addr as *const u8));
        println!(
            "read after munmap: {o:?}. In real programs the range may already be reused by another mapping, so the\n  access silently reads/corrupts someone else's data: never keep a pointer past munmap (Rust: tie lifetimes to the owner)."
        );
        assert!(segv(&o));
    }
}

fn main() {
    part_a();
    #[cfg(unix)]
    real::demo();
    #[cfg(not(unix))]
    println!(
        "Part B (real mmap/mprotect) is Unix-only; on Windows the equivalents are VirtualAlloc/VirtualProtect (see r241)."
    );
    println!("\nOK");
}
