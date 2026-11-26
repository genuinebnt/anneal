//! RECIPE 172 (memory): a toy general-purpose allocator: boundary tags, size-class free lists, splitting, coalescing, realloc.
//! LEVEL: advanced
//! Use in: understanding malloc/free, custom allocators, embedded heaps, fragmentation debugging.   Run: cargo run --release --example size_class_allocator
#![allow(dead_code)]
//!
//! The "heap" is a fixed `Vec<u8>` arena and pointers are byte OFFSETS into it, so the whole allocator is safe Rust (the real
//! thing hands out raw addresses; bump_arena_allocator shows the unsafe side). Every block looks like:
//!
//! ```text
//!   | header 8B: size|alloc | payload ... (free block: next, prev links live here) ... | footer 8B: size|alloc |
//! ```
//! * Sizes are multiples of 16 and INCLUDE the 16 bytes of tags; the first header sits at offset 8 so payloads are 16-aligned.
//!   Minimum block = 32 bytes (room for the two free-list links). `alloc(0)` still returns a unique 16-byte-aligned block.
//! * BOUNDARY TAGS: the footer copy of the header lets `free` look at the PREVIOUS block (read the 8 bytes before my header)
//!   in O(1), so adjacent free blocks are merged immediately (COALESCING). The invariant "no two adjacent free blocks" is what
//!   keeps external fragmentation from growing without bound, and it is checked below.
//! * SEGREGATED FREE LISTS: free blocks live in doubly linked lists (intrusive: the links are stored in the free payload)
//!   binned by SIZE CLASS: exact 16-byte steps from 32 to 496, then powers of two. `alloc` starts at the request's class and
//!   takes the first block that fits (first-fit within a class), SPLITTING off the remainder when it is at least 32 bytes.
//! * `realloc` shrinks in place (splitting), grows in place if the next block is free and large enough, else allocate-copy-free.
//! * FRAGMENTATION: INTERNAL = bytes inside allocated blocks not requested (rounding + tags); EXTERNAL = free memory that is
//!   scattered ("1 - largest free block / total free"): enough total space, yet a big request fails.
//!
//! HOW REAL ALLOCATORS IMPROVE ON THIS (general ideas, not a spec of any one implementation): they use many more size classes
//! with bounded waste so most requests need no splitting/coalescing; keep per-thread caches (tcmalloc's name says it; jemalloc
//! has thread caches too) so the common path takes no lock; use several arenas to reduce contention between threads; carve
//! small objects out of pages/runs dedicated to one size class so the header can be dropped and locality improves; and return
//! memory to the OS. mimalloc keeps small free lists per page ("free list sharding"). This toy has one lock-free-by-being-
//! single-threaded list set, a linear first-fit walk inside power-of-two bins, and a 16-byte tag overhead per block.
//!
//! COMMON USES: the concepts behind malloc in libc, embedded/no_std heaps (linked-list or TLSF allocators), game-engine
//! sub-allocators, GPU memory suballocation, and diagnosing "out of memory with plenty free" (external fragmentation).
//! Read: https://github.com/jemalloc/jemalloc and https://github.com/microsoft/mimalloc and https://github.com/google/tcmalloc
use std::collections::HashMap;

const HDR: usize = 8;
const MIN_BLOCK: usize = 32;
const NCLASS: usize = 48;
const NIL: u32 = 0; // payload offsets are >= 16, so 0 can mean "none"

struct Heap {
    mem: Vec<u8>,
    bins: [u32; NCLASS],         // head payload offset per size class
    first: usize,                // header offset of the first block
    end: usize,                  // one past the last block
    live: HashMap<usize, usize>, // payload offset -> requested size (for stats and double-free detection)
}

struct Report {
    free_bytes: usize,
    free_blocks: usize,
    largest_free: usize,
    internal_bytes: usize,
    allocated_block_bytes: usize,
}
impl Report {
    fn external_frag(&self) -> f64 {
        if self.free_bytes == 0 {
            0.0
        } else {
            1.0 - self.largest_free as f64 / self.free_bytes as f64
        }
    }
    fn internal_frag(&self) -> f64 {
        if self.allocated_block_bytes == 0 {
            0.0
        } else {
            self.internal_bytes as f64 / self.allocated_block_bytes as f64
        }
    }
}

fn class_of(size: usize) -> usize {
    if size < 512 {
        (size - MIN_BLOCK) / 16
    } else {
        (30 + size.ilog2() as usize - 9).min(NCLASS - 1)
    }
}
fn block_for(size: usize) -> Option<usize> {
    Some(
        size.checked_add(15)?
            .checked_div(16)?
            .checked_mul(16)?
            .checked_add(16)?
            .max(MIN_BLOCK),
    )
}

impl Heap {
    fn new(total: usize) -> Heap {
        assert!(total % 16 == 0 && total >= 64);
        let mut h = Heap {
            mem: vec![0; total],
            bins: [NIL; NCLASS],
            first: 8,
            end: total - 8,
            live: HashMap::new(),
        };
        let (f, n) = (h.first, h.end - h.first);
        h.set_block(f, n, false);
        h.bin_insert(f);
        h
    }
    fn rd(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.mem[at..at + 8].try_into().unwrap())
    }
    fn wr(&mut self, at: usize, v: u64) {
        self.mem[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    fn rd32(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.mem[at..at + 4].try_into().unwrap())
    }
    fn wr32(&mut self, at: usize, v: u32) {
        self.mem[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn size_at(&self, h: usize) -> usize {
        (self.rd(h) & !15) as usize
    }
    fn is_alloc(&self, h: usize) -> bool {
        self.rd(h) & 1 == 1
    }
    fn set_block(&mut self, h: usize, size: usize, alloc: bool) {
        let w = size as u64 | alloc as u64;
        self.wr(h, w);
        self.wr(h + size - HDR, w); // footer = boundary tag
    }

    fn bin_insert(&mut self, h: usize) {
        let (p, c) = (h + HDR, class_of(self.size_at(h)));
        let head = self.bins[c];
        self.wr32(p, head); // next
        self.wr32(p + 4, NIL); // prev
        if head != NIL {
            self.wr32(head as usize + 4, p as u32);
        }
        self.bins[c] = p as u32;
    }
    fn bin_remove(&mut self, h: usize) {
        let (p, c) = (h + HDR, class_of(self.size_at(h)));
        let (next, prev) = (self.rd32(p), self.rd32(p + 4));
        if prev != NIL {
            self.wr32(prev as usize, next)
        } else {
            self.bins[c] = next
        }
        if next != NIL {
            self.wr32(next as usize + 4, prev);
        }
    }

    /// Turn the free region `[h, h+size)` (not in any bin) into a free block, merging with a free block that follows it.
    fn free_region(&mut self, h: usize, mut size: usize) {
        let nh = h + size;
        if nh < self.end && !self.is_alloc(nh) {
            self.bin_remove(nh);
            size += self.size_at(nh);
        }
        self.set_block(h, size, false);
        self.bin_insert(h);
    }
    /// Mark `[h, h+region)` allocated with `need` bytes, splitting off a tail if it can be a block of its own.
    fn make_alloc(&mut self, h: usize, region: usize, need: usize) {
        if region - need >= MIN_BLOCK {
            self.set_block(h, need, true);
            self.free_region(h + need, region - need);
        } else {
            self.set_block(h, region, true);
        }
    }

    fn alloc(&mut self, size: usize) -> Option<usize> {
        let need = block_for(size)?;
        for c in class_of(need)..NCLASS {
            let mut p = self.bins[c];
            while p != NIL {
                let h = p as usize - HDR;
                let bsize = self.size_at(h);
                if bsize >= need {
                    self.bin_remove(h);
                    self.make_alloc(h, bsize, need);
                    self.live.insert(h + HDR, size);
                    return Some(h + HDR);
                }
                p = self.rd32(p as usize);
            }
        }
        None
    }

    fn free(&mut self, p: usize) {
        assert!(
            self.live.remove(&p).is_some(),
            "invalid or double free of {p}"
        );
        let (mut h, mut size) = (p - HDR, self.size_at(p - HDR));
        assert!(self.is_alloc(h));
        if h > self.first && self.rd(h - HDR) & 1 == 0 {
            let psize = (self.rd(h - HDR) & !15) as usize; // previous block's footer tells its size
            h -= psize;
            self.bin_remove(h);
            size += psize;
        }
        self.free_region(h, size); // merges with the next block too
    }

    fn realloc(&mut self, p: usize, new_size: usize) -> Option<usize> {
        let need = block_for(new_size)?;
        let h = p - HDR;
        let bs = self.size_at(h);
        assert!(self.live.contains_key(&p), "realloc of dead pointer");
        if need <= bs {
            self.make_alloc(h, bs, need); // shrink in place, tail returned (and merged)
        } else {
            let nh = h + bs;
            if nh < self.end && !self.is_alloc(nh) && bs + self.size_at(nh) >= need {
                let region = bs + self.size_at(nh);
                self.bin_remove(nh);
                self.make_alloc(h, region, need); // grow in place
            } else {
                let old = self.live[&p];
                let q = self.alloc(new_size)?;
                self.mem.copy_within(p..p + old.min(new_size), q);
                self.free(p);
                return Some(q);
            }
        }
        self.live.insert(p, new_size);
        Some(p)
    }

    fn bytes_mut(&mut self, p: usize) -> &mut [u8] {
        let n = self.live[&p];
        &mut self.mem[p..p + n]
    }
    fn bytes(&self, p: usize) -> &[u8] {
        &self.mem[p..p + self.live[&p]]
    }

    fn report(&self) -> Report {
        let mut r = Report {
            free_bytes: 0,
            free_blocks: 0,
            largest_free: 0,
            internal_bytes: 0,
            allocated_block_bytes: 0,
        };
        let mut h = self.first;
        while h < self.end {
            let s = self.size_at(h);
            if self.is_alloc(h) {
                r.allocated_block_bytes += s;
                r.internal_bytes += s - self.live[&(h + HDR)];
            } else {
                r.free_bytes += s;
                r.free_blocks += 1;
                r.largest_free = r.largest_free.max(s);
            }
            h += s;
        }
        r
    }

    /// Structural invariants: tags agree, blocks tile the arena, no adjacent free blocks, bins hold exactly the free blocks.
    fn check(&self) {
        let (mut h, mut prev_free, mut free_blocks) = (self.first, false, 0);
        while h < self.end {
            let s = self.size_at(h);
            assert!(
                s >= MIN_BLOCK && s % 16 == 0 && (h + HDR) % 16 == 0 && h + s <= self.end,
                "bad block at {h}"
            );
            assert_eq!(
                self.rd(h),
                self.rd(h + s - HDR),
                "header/footer mismatch at {h}"
            );
            let free = !self.is_alloc(h);
            assert!(
                !(free && prev_free),
                "adjacent free blocks at {h}: coalescing failed"
            );
            assert_eq!(
                !free,
                self.live.contains_key(&(h + HDR)),
                "live map disagrees at {h}"
            );
            free_blocks += free as usize;
            prev_free = free;
            h += s;
        }
        assert_eq!(h, self.end);
        let mut in_bins = 0;
        for c in 0..NCLASS {
            let (mut p, mut prev) = (self.bins[c], NIL);
            while p != NIL {
                let bh = p as usize - HDR;
                assert!(
                    !self.is_alloc(bh) && class_of(self.size_at(bh)) == c,
                    "wrong block in bin {c}"
                );
                assert_eq!(self.rd32(p as usize + 4), prev, "broken prev link");
                prev = p;
                p = self.rd32(p as usize);
                in_bins += 1;
            }
        }
        assert_eq!(in_bins, free_blocks, "bins and heap walk disagree");
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn main() {
    // 1. basics: split, coalesce, reuse
    let mut h = Heap::new(4096);
    let total = h.report().free_bytes;
    let (a, b, c) = (
        h.alloc(100).unwrap(),
        h.alloc(100).unwrap(),
        h.alloc(100).unwrap(),
    );
    assert!([a, b, c].iter().all(|p| p % 16 == 0));
    h.free(b);
    h.free(a); // merges with the free b (previous-neighbour boundary tag lookup)
    h.check();
    assert_eq!(h.report().free_blocks, 2);
    h.free(c); // merges both sides
    h.check();
    assert_eq!((h.report().free_blocks, h.report().free_bytes), (1, total));
    assert!(h.alloc(1 << 20).is_none() && h.alloc(usize::MAX).is_none());
    println!("basic: split on alloc, coalesced back to 1 free block of {total} bytes");

    // 2. realloc: shrink, grow in place, grow by move (content preserved)
    let p = h.alloc(64).unwrap();
    h.bytes_mut(p).fill(0xAB);
    let p2 = h.realloc(p, 600).unwrap();
    assert_eq!(p2, p, "next block is free: grows in place");
    assert!(h.bytes(p2)[..64].iter().all(|&b| b == 0xAB));
    let blocker = h.alloc(10).unwrap();
    let p3 = h.realloc(p2, 2000).unwrap();
    assert_ne!(p3, p2, "blocked by a neighbour: must move");
    assert!(h.bytes(p3)[..64].iter().all(|&b| b == 0xAB));
    let p4 = h.realloc(p3, 40).unwrap();
    assert_eq!(p4, p3);
    h.free(p4);
    h.free(blocker);
    h.check();
    assert_eq!(h.report().free_blocks, 1);
    println!("realloc: in-place grow, move-on-grow, in-place shrink all keep contents");

    // 3. external fragmentation: plenty of free memory, but not contiguous
    let mut h = Heap::new(128 * 1024);
    let mut ps = Vec::new();
    while let Some(p) = h.alloc(1000) {
        ps.push(p);
    }
    let n = ps.len() - 1; // keep the last block allocated so the free tail does not merge into a big hole
    for p in ps[..n].iter().step_by(2) {
        h.free(*p);
    }
    h.check();
    let r = h.report();
    assert!(
        h.alloc(2000).is_none(),
        "free memory is fragmented into 1 KiB holes"
    );
    println!(
        "fragmentation: {} free bytes in {} holes (largest {}): external {:.0}%, internal {:.1}% of allocated; alloc(2000) -> None",
        r.free_bytes,
        r.free_blocks,
        r.largest_free,
        100.0 * r.external_frag(),
        100.0 * r.internal_frag()
    );
    assert!(r.external_frag() > 0.9);
    for (i, p) in ps.iter().enumerate() {
        if i >= n || i % 2 == 1 {
            h.free(*p);
        }
    }
    h.check();
    assert_eq!(h.report().free_blocks, 1);
    assert!(h.alloc(2000).is_some());

    // 4. seeded random stress: no overlap, no corruption, full coalescing at the end
    let mut h = Heap::new(1 << 20);
    let total = h.report().free_bytes;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut live: Vec<(usize, usize, u8)> = Vec::new(); // payload offset, requested size, pattern
    let (mut fails, mut reallocs, mut peak) = (0, 0, 0usize);
    for step in 0..200_000u32 {
        let verify = |h: &Heap, &(p, n, tag): &(usize, usize, u8)| {
            assert_eq!(h.bytes(p).len(), n);
            assert!(
                h.bytes(p).iter().all(|&b| b == tag),
                "corruption in block {p} (step {step})"
            );
        };
        let size = if rng.below(50) == 0 {
            2000 + rng.below(20_000)
        } else {
            rng.below(600)
        };
        match rng.below(10) {
            0..=4 => match h.alloc(size) {
                Some(p) => {
                    assert!(p % 16 == 0);
                    let tag = (step % 251) as u8 + 1;
                    h.bytes_mut(p).fill(tag);
                    live.push((p, size, tag));
                }
                None => fails += 1,
            },
            5..=8 if !live.is_empty() => {
                let i = rng.below(live.len());
                verify(&h, &live[i]);
                h.free(live.swap_remove(i).0);
            }
            _ if !live.is_empty() => {
                let i = rng.below(live.len());
                verify(&h, &live[i]);
                let (p, n, tag) = live[i];
                if let Some(q) = h.realloc(p, size) {
                    assert!(
                        h.bytes(q)[..n.min(size)].iter().all(|&b| b == tag),
                        "realloc lost data"
                    );
                    h.bytes_mut(q).fill(tag);
                    live[i] = (q, size, tag);
                    reallocs += 1;
                } // on failure the old block must stay valid and intact
                verify(&h, &live[i]);
            }
            _ => {}
        }
        if step % 4_000 == 0 {
            h.check();
            let mut iv: Vec<(usize, usize)> = live.iter().map(|&(p, n, _)| (p, p + n)).collect();
            iv.sort();
            assert!(
                iv.windows(2).all(|w| w[0].1 <= w[1].0),
                "overlapping allocations"
            );
            peak = peak.max(live.len());
        }
    }
    h.check();
    let r = h.report();
    println!(
        "stress: 200000 ops, {} live (peak sampled {peak}), {fails} failed allocs, {reallocs} reallocs; external {:.0}% internal {:.1}%",
        live.len(),
        100.0 * r.external_frag(),
        100.0 * r.internal_frag()
    );
    for t in &live {
        assert!(h.bytes(t.0).iter().all(|&b| b == t.2), "corruption at end");
    }
    for (p, _, _) in live.drain(..) {
        h.free(p);
    }
    h.check();
    let r = h.report();
    assert_eq!(
        (r.free_blocks, r.free_bytes, r.largest_free),
        (1, total, total),
        "must coalesce to one block"
    );
    println!("after freeing everything: 1 free block of {total} bytes");
    println!("ok");
}
