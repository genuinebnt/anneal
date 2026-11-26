//! RECIPE 420 (memory): garbage collection algorithms on a simulated managed heap, checked by a reachability oracle.
//! LEVEL: advanced
//! Use in: language runtimes (JVM/Go/V8/CPython-style), scripting engines, game engines, any system with a managed heap.   Run: cargo run --release --example garbage_collectors
//!
//! WHAT: five classic collectors over one simulated heap model: objects have a header (live flag, colour/mark,
//! age, remembered bit, refcount, forwarding slot), a type tag, a payload word and NF reference fields; they live in
//! a `Vec` arena and are addressed by `u32` handles (`Ref`), with a root-slot array playing the role of the stack.
//!   (a) MARK-SWEEP with an explicit mark stack (no recursion, so a 100k-long list cannot overflow the call stack),
//!       an intrusive FREE LIST threaded through dead objects, and a sweep pass.
//!   (b) TRI-COLOUR INCREMENTAL marking (white = unvisited, grey = queued, black = scanned) interleaved with the
//!       mutator, protected by Dijkstra's INSERTION WRITE BARRIER (shade the target of every pointer store). We
//!       show the lost-object bug when the barrier is switched off. Roots are re-scanned at termination because
//!       stack writes carry no barrier (the original Dijkstra design; Go used this before its hybrid barrier).
//!   (c) CHENEY SEMISPACE COPYING: evacuate roots into to-space, then a scan pointer chases the copy frontier
//!       (a BFS queue that lives inside the to-space itself); FORWARDING pointers keep sharing and cycles intact.
//!       Output is compacted: allocation becomes a bump, and dead objects cost nothing (work is proportional to LIVE data).
//!   (d) GENERATIONAL: nursery + old generation. Survivors age; reaching PROMOTE_AGE copies them to the old space.
//!       Old objects are not traced in a minor collection, so an old->young pointer would be invisible: the WRITE
//!       BARRIER records the old object in a REMEMBERED SET, which a minor GC treats as extra roots.
//!   (e) REFERENCE COUNTING: immediate reclamation (no pauses beyond cascades), but cycles leak by design. A
//!       backup cycle collector using TRIAL DELETION (subtract internal references; what remains is externally
//!       referenced, trace from it; the rest is cyclic garbage, as in CPython's gc module) reclaims them.
//!
//! WHY the oracle: a GC bug is silent corruption, so every collector is driven by the same seeded RANDOM MUTATOR
//! (allocate, link, unlink, make cycles, drop/copy roots, mutate payloads) that also updates a plain model graph
//! which never frees anything. After each collection we assert (1) every reachable object survives with the same
//! tag/payload and an ISOMORPHIC graph (sharing and cycles preserved, even when objects moved), (2) for exact
//! collections the live count equals the reachable count (RC: reachable + exactly the cyclic garbage), and (3)
//! heap invariants (free list sane, no dangling pointers, tri-colour invariant, remembered-set coverage, RC
//! equals real in-degree). Payloads are unique per allocation so a pointer to a recycled slot is caught.
//!
//! RUST VS THESE SCHEMES: Rust does memory management at COMPILE time. Ownership gives every value one owner,
//! `Drop` runs exactly where the owner dies, and the borrow checker proves no reference outlives its referent, so there is
//! no collector, no pause and no header on the object. The price is that graphs with sharing and cycles need
//! indices (like this file's arena), `Rc<RefCell<..>>` (this file's scheme (e), including its cycle leak, fixed
//! by `Weak`), or a GC library. A tracing runtime pays at RUN time (barriers, pauses, 2x space for copying) to let
//! programs build any graph freely.
//!
//! COMMON USES: the JVM's generational collectors (nursery copying, old-gen mark/compact, card-marking
//! barriers), Go's concurrent tri-colour mark-sweep with write barriers, CPython's refcounting plus a cyclic
//! collector, Lua/JavaScript engines (incremental/generational marking), Lisp/ML runtimes (Cheney-style
//! copying nursery), Swift/ObjC ARC (RC, cycles broken by weak refs).
//! Read: "The Garbage Collection Handbook" (Jones, Hosking, Moss) https://gchandbook.org/   "The Rust Programming
//! Language", ch. 4 https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html
#![allow(dead_code)]
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

// ---------- object model ----------
type Ref = u32;
const NIL: Ref = u32::MAX;
const NF: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Color {
    White,
    Grey,
    Black,
}

/// Object header + body. One struct serves all collectors; each uses the header bits it needs.
#[derive(Clone, Debug)]
struct Obj {
    live: bool,
    color: Color, // mark bit (mark-sweep) or tri-colour (incremental)
    age: u8,      // survivals of minor collections (generational)
    remembered: bool,
    rc: u32,  // reference count (RC)
    fwd: Ref, // forwarding pointer (copying collectors)
    tag: u8,
    payload: u64,
    fields: [Ref; NF],
}

impl Obj {
    fn new(tag: u8, payload: u64) -> Obj {
        Obj {
            live: true,
            color: Color::White,
            age: 0,
            remembered: false,
            rc: 0,
            fwd: NIL,
            tag,
            payload,
            fields: [NIL; NF],
        }
    }
}

/// What the mutator/oracle may observe of an object.
#[derive(Clone, Copy, Debug)]
struct View {
    tag: u8,
    payload: u64,
    fields: [Ref; NF],
}

impl View {
    fn of(o: &Obj) -> View {
        View {
            tag: o.tag,
            payload: o.payload,
            fields: o.fields,
        }
    }
}

trait Heap {
    fn name(&self) -> &'static str;
    fn alloc(&mut self, tag: u8, payload: u64) -> Ref;
    /// None means the handle is dangling (freed or out of range): the bug every test hunts for.
    fn view(&self, r: Ref) -> Option<View>;
    /// Pointer store; this is where write barriers live.
    fn set_field(&mut self, r: Ref, i: usize, v: Ref);
    fn set_payload(&mut self, r: Ref, p: u64);
    /// Root stores go through the heap so RC can count them; tracing collectors just assign.
    fn set_root(&mut self, roots: &mut [Ref], slot: usize, v: Ref) {
        roots[slot] = v;
    }
    /// Called after every mutator operation; may run a bounded slice of GC work. True when a cycle just completed.
    fn tick(&mut self, roots: &mut [Ref]) -> bool;
    /// Finish any cycle in flight (no-op for stop-the-world collectors).
    fn settle(&mut self, _roots: &mut [Ref]) {}
    /// Stop-the-world full collection; afterwards live == reachable (RC: cyclic garbage reclaimed).
    fn collect(&mut self, roots: &mut [Ref]);
    fn live_objects(&self) -> usize;
    fn check_invariants(&self, roots: &[Ref]) -> Result<(), String>;
    fn pauses(&self) -> &[usize];
    /// Is the heap exactly the reachable set when `tick` reports completion?
    fn exact_after_tick(&self) -> bool {
        true
    }
    /// RC without a cycle collector keeps cyclic garbage forever.
    fn leaks_cycles(&self) -> bool {
        false
    }
}

// ---------- shared slot arena with an intrusive free list ----------
struct Arena {
    objs: Vec<Obj>,
    free_head: Ref,
    live: usize,
}

impl Arena {
    fn new() -> Arena {
        Arena {
            objs: Vec::new(),
            free_head: NIL,
            live: 0,
        }
    }
    /// Reuse the free-list head if any: allocation is O(1) pop, and recycled slots are why dangling handles matter.
    fn alloc(&mut self, o: Obj) -> Ref {
        self.live += 1;
        if self.free_head != NIL {
            let r = self.free_head;
            self.free_head = self.objs[r as usize].fields[0];
            self.objs[r as usize] = o;
            r
        } else {
            self.objs.push(o);
            (self.objs.len() - 1) as Ref
        }
    }
    fn free(&mut self, r: Ref) {
        let next = self.free_head;
        let o = &mut self.objs[r as usize];
        o.live = false;
        o.fields = [NIL; NF];
        o.fields[0] = next; // dead objects store the free-list link in their first field
        self.free_head = r;
        self.live -= 1;
    }
    fn get(&self, r: Ref) -> Option<&Obj> {
        self.objs.get(r as usize).filter(|o| o.live)
    }
    fn check(&self, roots: &[Ref]) -> Result<(), String> {
        let mut n = 0;
        let mut r = self.free_head;
        while r != NIL {
            let o = self
                .objs
                .get(r as usize)
                .ok_or("free list index out of range")?;
            if o.live {
                return Err("live object on the free list".into());
            }
            n += 1;
            if n > self.objs.len() {
                return Err("free list has a cycle".into());
            }
            r = o.fields[0];
        }
        if n + self.live != self.objs.len() {
            return Err(format!(
                "free {} + live {} != slots {}",
                n,
                self.live,
                self.objs.len()
            ));
        }
        let valid = |r: Ref| r == NIL || self.get(r).is_some();
        if !roots.iter().all(|&r| valid(r)) {
            return Err("dangling root".into());
        }
        if self
            .objs
            .iter()
            .filter(|o| o.live)
            .any(|o| !o.fields.iter().all(|&f| valid(f)))
        {
            return Err("live object points at a freed slot".into());
        }
        Ok(())
    }
}

// ---------- (a) mark-sweep ----------
struct MarkSweep {
    a: Arena,
    stack: Vec<Ref>,
    since: usize,
    trigger: usize,
    pauses: Vec<usize>,
}

impl MarkSweep {
    fn new() -> Self {
        MarkSweep {
            a: Arena::new(),
            stack: Vec::new(),
            since: 0,
            trigger: 128,
            pauses: Vec::new(),
        }
    }
    fn mark_from(&mut self, r: Ref) {
        if r != NIL && self.a.objs[r as usize].color == Color::White {
            self.a.objs[r as usize].color = Color::Black;
            self.stack.push(r);
        }
    }
}

impl Heap for MarkSweep {
    fn name(&self) -> &'static str {
        "mark-sweep"
    }
    fn alloc(&mut self, tag: u8, payload: u64) -> Ref {
        self.since += 1;
        self.a.alloc(Obj::new(tag, payload))
    }
    fn view(&self, r: Ref) -> Option<View> {
        self.a.get(r).map(View::of)
    }
    fn set_field(&mut self, r: Ref, i: usize, v: Ref) {
        self.a.objs[r as usize].fields[i] = v;
    }
    fn set_payload(&mut self, r: Ref, p: u64) {
        self.a.objs[r as usize].payload = p;
    }
    fn tick(&mut self, roots: &mut [Ref]) -> bool {
        if self.since < self.trigger {
            return false;
        }
        self.collect(roots);
        true
    }
    fn collect(&mut self, roots: &mut [Ref]) {
        for &r in roots.iter() {
            self.mark_from(r);
        }
        let mut marked = 0;
        // Marking uses an explicit stack: depth is bounded by heap size, not by the call stack.
        while let Some(r) = self.stack.pop() {
            marked += 1;
            let fs = self.a.objs[r as usize].fields;
            for f in fs {
                self.mark_from(f);
            }
        }
        for i in 0..self.a.objs.len() {
            let o = &mut self.a.objs[i];
            if !o.live {
                continue;
            }
            if o.color == Color::Black {
                o.color = Color::White; // reset for the next cycle
            } else {
                self.a.free(i as Ref);
            }
        }
        self.pauses.push(marked + self.a.objs.len());
        self.since = 0;
        self.trigger = self.a.live.max(128);
    }
    fn live_objects(&self) -> usize {
        self.a.live
    }
    fn check_invariants(&self, roots: &[Ref]) -> Result<(), String> {
        self.a.check(roots)?;
        if self
            .a
            .objs
            .iter()
            .any(|o| o.live && o.color != Color::White)
        {
            return Err("mark bit left set after sweep".into());
        }
        Ok(())
    }
    fn pauses(&self) -> &[usize] {
        &self.pauses
    }
}

// ---------- (b) incremental tri-colour with Dijkstra barrier ----------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Idle,
    Marking,
    Sweeping(usize),
}

struct Incremental {
    a: Arena,
    barrier: bool,
    phase: Phase,
    grey: Vec<Ref>,
    since: usize,
    trigger: usize,
    budget: usize,
    pauses: Vec<usize>,
}

impl Incremental {
    fn new(barrier: bool) -> Self {
        Incremental {
            a: Arena::new(),
            barrier,
            phase: Phase::Idle,
            grey: Vec::new(),
            since: 0,
            trigger: 128,
            budget: 8,
            pauses: Vec::new(),
        }
    }
    /// white -> grey: queue for scanning.
    fn shade(&mut self, r: Ref) {
        if r != NIL && self.a.objs[r as usize].color == Color::White {
            self.a.objs[r as usize].color = Color::Grey;
            self.grey.push(r);
        }
    }
    fn begin(&mut self, roots: &[Ref]) {
        self.phase = Phase::Marking;
        for &r in roots {
            self.shade(r);
        }
        self.pauses.push(roots.len());
    }
    /// Scan up to `n` grey objects (grey -> black). This is one bounded increment of marking.
    fn mark_step(&mut self, n: usize) -> usize {
        let mut done = 0;
        while done < n {
            let Some(r) = self.grey.pop() else { break };
            let fs = self.a.objs[r as usize].fields;
            for f in fs {
                self.shade(f);
            }
            self.a.objs[r as usize].color = Color::Black;
            done += 1;
        }
        done
    }
    /// Termination: stack writes are unbarriered, so re-scan roots, then drain everything grey.
    fn finish_mark(&mut self, roots: &[Ref]) -> usize {
        for &r in roots {
            self.shade(r);
        }
        let n = self.mark_step(usize::MAX);
        self.phase = Phase::Sweeping(0);
        n + roots.len()
    }
    /// Sweep up to `n` slots; returns true when the sweep is complete.
    fn sweep_step(&mut self, n: usize) -> bool {
        let Phase::Sweeping(mut cur) = self.phase else {
            return true;
        };
        for _ in 0..n {
            if cur >= self.a.objs.len() {
                break;
            }
            let o = &mut self.a.objs[cur];
            if o.live {
                if o.color == Color::White {
                    self.a.free(cur as Ref);
                } else {
                    o.color = Color::White;
                }
            }
            cur += 1;
        }
        if cur >= self.a.objs.len() {
            self.phase = Phase::Idle;
            self.since = 0;
            self.trigger = self.a.live.max(128);
            true
        } else {
            self.phase = Phase::Sweeping(cur);
            false
        }
    }
}

impl Heap for Incremental {
    fn name(&self) -> &'static str {
        if self.barrier {
            "incremental+barrier"
        } else {
            "incremental NO barrier"
        }
    }
    fn alloc(&mut self, tag: u8, payload: u64) -> Ref {
        self.since += 1;
        let r = self.a.alloc(Obj::new(tag, payload));
        // Allocate black while marking (a new object must not be freed by this cycle) and, during the sweep,
        // black only if the sweep cursor has not reached its slot yet (the sweep will reset it to white).
        let black = match self.phase {
            Phase::Idle => false,
            Phase::Marking => true,
            Phase::Sweeping(cur) => (r as usize) >= cur,
        };
        if black {
            self.a.objs[r as usize].color = Color::Black;
        }
        r
    }
    fn view(&self, r: Ref) -> Option<View> {
        self.a.get(r).map(View::of)
    }
    fn set_field(&mut self, r: Ref, i: usize, v: Ref) {
        if self.barrier && self.phase == Phase::Marking {
            self.shade(v); // Dijkstra insertion barrier: never let a black object point at a white one
        }
        self.a.objs[r as usize].fields[i] = v;
    }
    fn set_payload(&mut self, r: Ref, p: u64) {
        self.a.objs[r as usize].payload = p;
    }
    fn tick(&mut self, roots: &mut [Ref]) -> bool {
        match self.phase {
            Phase::Idle => {
                if self.since >= self.trigger {
                    self.begin(roots);
                }
                false
            }
            Phase::Marking => {
                let n = self.mark_step(self.budget);
                self.pauses.push(n);
                if self.grey.is_empty() {
                    let n = self.finish_mark(roots);
                    self.pauses.push(n);
                }
                false
            }
            Phase::Sweeping(_) => {
                self.pauses.push(self.budget * 4);
                self.sweep_step(self.budget * 4)
            }
        }
    }
    fn settle(&mut self, roots: &mut [Ref]) {
        if self.phase == Phase::Marking {
            self.finish_mark(roots);
        }
        self.sweep_step(usize::MAX);
    }
    fn collect(&mut self, roots: &mut [Ref]) {
        if self.phase == Phase::Idle {
            self.begin(roots);
        }
        let n = if self.phase == Phase::Marking {
            self.finish_mark(roots)
        } else {
            0
        };
        self.sweep_step(usize::MAX);
        self.pauses.push(n + self.a.objs.len());
    }
    fn live_objects(&self) -> usize {
        self.a.live
    }
    fn check_invariants(&self, roots: &[Ref]) -> Result<(), String> {
        self.a.check(roots)?;
        let live = self.a.objs.iter().filter(|o| o.live);
        match self.phase {
            Phase::Idle if live.clone().any(|o| o.color != Color::White) => {
                Err("non-white object while idle".into())
            }
            Phase::Marking => {
                // The strong tri-colour invariant: no black object points at a white one.
                for o in live.filter(|o| o.color == Color::Black) {
                    if o.fields
                        .iter()
                        .any(|&f| f != NIL && self.a.objs[f as usize].color == Color::White)
                    {
                        return Err("tri-colour invariant broken: black -> white".into());
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn pauses(&self) -> &[usize] {
        &self.pauses
    }
    fn exact_after_tick(&self) -> bool {
        false // objects that died during the cycle float until the next one
    }
}

// ---------- (c) Cheney semispace copying ----------
struct Cheney {
    space: Vec<Obj>,
    trigger: usize,
    pauses: Vec<usize>,
}

/// Copy `r` to to-space unless already copied; either way return its new address.
fn forward(from: &mut [Obj], r: Ref, to: &mut Vec<Obj>) -> Ref {
    let o = &mut from[r as usize];
    if o.fwd != NIL {
        return o.fwd; // already evacuated: this is what preserves sharing and cycles
    }
    let mut c = o.clone();
    c.fwd = NIL;
    to.push(c);
    let new = (to.len() - 1) as Ref;
    o.fwd = new;
    new
}

impl Cheney {
    fn new() -> Self {
        Cheney {
            space: Vec::new(),
            trigger: 128,
            pauses: Vec::new(),
        }
    }
}

impl Heap for Cheney {
    fn name(&self) -> &'static str {
        "cheney-copying"
    }
    fn alloc(&mut self, tag: u8, payload: u64) -> Ref {
        self.space.push(Obj::new(tag, payload)); // bump allocation
        (self.space.len() - 1) as Ref
    }
    fn view(&self, r: Ref) -> Option<View> {
        self.space.get(r as usize).map(View::of)
    }
    fn set_field(&mut self, r: Ref, i: usize, v: Ref) {
        self.space[r as usize].fields[i] = v;
    }
    fn set_payload(&mut self, r: Ref, p: u64) {
        self.space[r as usize].payload = p;
    }
    fn tick(&mut self, roots: &mut [Ref]) -> bool {
        if self.space.len() < self.trigger {
            return false;
        }
        self.collect(roots);
        true
    }
    fn collect(&mut self, roots: &mut [Ref]) {
        let mut to: Vec<Obj> = Vec::with_capacity(self.space.len() / 2);
        for r in roots.iter_mut() {
            if *r != NIL {
                *r = forward(&mut self.space, *r, &mut to);
            }
        }
        // `scan` chases `to.len()`: everything below scan is fully updated, above it holds old-space pointers.
        let mut scan = 0;
        while scan < to.len() {
            let mut fs = to[scan].fields;
            for f in fs.iter_mut() {
                if *f != NIL {
                    *f = forward(&mut self.space, *f, &mut to);
                }
            }
            to[scan].fields = fs;
            scan += 1;
        }
        self.pauses.push(to.len()); // work is proportional to live data only
        self.space = to; // flip: the old space is dropped wholesale
        self.trigger = (self.space.len() * 2).max(128);
    }
    fn live_objects(&self) -> usize {
        self.space.len()
    }
    fn check_invariants(&self, roots: &[Ref]) -> Result<(), String> {
        let n = self.space.len() as Ref;
        if roots.iter().any(|&r| r != NIL && r >= n) {
            return Err("root outside the space".into());
        }
        if self
            .space
            .iter()
            .any(|o| o.fwd != NIL || o.fields.iter().any(|&f| f != NIL && f >= n))
        {
            return Err("stale forwarding pointer or out-of-space field".into());
        }
        Ok(())
    }
    fn pauses(&self) -> &[usize] {
        &self.pauses
    }
}

// ---------- (d) generational with remembered set ----------
const OLD: Ref = 1 << 31; // old-space handles carry this bit (NIL is checked first everywhere)
fn is_old(r: Ref) -> bool {
    r != NIL && r & OLD != 0
}
fn is_young(r: Ref) -> bool {
    r != NIL && r & OLD == 0
}

struct Generational {
    nursery: Vec<Obj>,
    old: Arena,
    rset: Vec<Ref>,
    barrier: bool,
    nursery_cap: usize,
    promote_age: u8,
    old_trigger: usize,
    pauses: Vec<usize>,
    minors: usize,
    fulls: usize,
}

impl Generational {
    fn new(barrier: bool) -> Self {
        Generational {
            nursery: Vec::new(),
            old: Arena::new(),
            rset: Vec::new(),
            barrier,
            nursery_cap: 64,
            promote_age: 2,
            old_trigger: 256,
            pauses: Vec::new(),
            minors: 0,
            fulls: 0,
        }
    }
    fn obj(&self, r: Ref) -> Option<&Obj> {
        if r == NIL {
            None
        } else if is_old(r) {
            self.old.get(r & !OLD)
        } else {
            self.nursery.get(r as usize)
        }
    }
    fn obj_mut(&mut self, r: Ref) -> &mut Obj {
        if is_old(r) {
            &mut self.old.objs[(r & !OLD) as usize]
        } else {
            &mut self.nursery[r as usize]
        }
    }
    /// Copy a nursery object to the survivor space (`to`) or promote it; old/NIL refs pass through unchanged.
    fn evac(&mut self, r: Ref, to: &mut Vec<Obj>, promoted: &mut Vec<Ref>, promote_age: u8) -> Ref {
        if !is_young(r) {
            return r;
        }
        let o = &mut self.nursery[r as usize];
        if o.fwd != NIL {
            return o.fwd;
        }
        let mut c = o.clone();
        c.fwd = NIL;
        c.age = c.age.saturating_add(1);
        let new = if c.age >= promote_age {
            c.remembered = false;
            let n = self.old.alloc(c) | OLD;
            promoted.push(n);
            n
        } else {
            to.push(c);
            (to.len() - 1) as Ref
        };
        self.nursery[r as usize].fwd = new;
        new
    }
    /// Minor collection: roots + remembered set are the only entry points into the nursery.
    fn minor(&mut self, roots: &mut [Ref], promote_age: u8) {
        let mut to: Vec<Obj> = Vec::new();
        let mut promoted: Vec<Ref> = Vec::new();
        let mut scanned = 0;
        for r in roots.iter_mut() {
            *r = self.evac(*r, &mut to, &mut promoted, promote_age);
        }
        for o in std::mem::take(&mut self.rset) {
            self.obj_mut(o).remembered = false;
            promoted.push(o); // scan remembered old objects like freshly promoted ones
        }
        let mut new_rset = Vec::new();
        let mut scan = 0;
        loop {
            if scan < to.len() {
                let mut fs = to[scan].fields;
                for f in fs.iter_mut() {
                    *f = self.evac(*f, &mut to, &mut promoted, promote_age);
                }
                to[scan].fields = fs;
                scan += 1;
            } else if let Some(o) = promoted.pop() {
                let mut fs = self.obj(o).expect("old object").fields;
                for f in fs.iter_mut() {
                    *f = self.evac(*f, &mut to, &mut promoted, promote_age);
                }
                let ob = self.obj_mut(o);
                ob.fields = fs;
                // An old object that still points at a surviving young object must stay remembered.
                if fs.iter().any(|&f| is_young(f)) && !ob.remembered {
                    ob.remembered = true;
                    new_rset.push(o);
                }
            } else {
                break;
            }
            scanned += 1;
        }
        self.nursery = to;
        self.rset = new_rset;
        self.minors += 1;
        self.pauses.push(scanned);
    }
    /// Full collection: promote everything, then mark-sweep the old space.
    fn full(&mut self, roots: &mut [Ref]) {
        self.minor(roots, 0);
        let mut stack: Vec<Ref> = Vec::new();
        for &r in roots.iter() {
            self.mark_old(r, &mut stack);
        }
        let mut marked = 0;
        while let Some(r) = stack.pop() {
            marked += 1;
            for f in self.obj(r).expect("marked").fields {
                self.mark_old(f, &mut stack);
            }
        }
        for i in 0..self.old.objs.len() {
            let o = &mut self.old.objs[i];
            if o.live && o.color == Color::Black {
                o.color = Color::White;
            } else if o.live {
                self.old.free(i as Ref);
            }
        }
        self.fulls += 1;
        self.old_trigger = (self.old.live * 2).max(256);
        self.pauses.push(marked + self.old.objs.len());
    }
    fn mark_old(&mut self, r: Ref, stack: &mut Vec<Ref>) {
        if is_old(r) && self.obj(r).is_some_and(|o| o.color == Color::White) {
            self.obj_mut(r).color = Color::Black;
            stack.push(r);
        }
    }
}

impl Heap for Generational {
    fn name(&self) -> &'static str {
        if self.barrier {
            "generational+rset"
        } else {
            "generational NO barrier"
        }
    }
    fn alloc(&mut self, tag: u8, payload: u64) -> Ref {
        self.nursery.push(Obj::new(tag, payload));
        (self.nursery.len() - 1) as Ref
    }
    fn view(&self, r: Ref) -> Option<View> {
        self.obj(r).map(View::of)
    }
    fn set_field(&mut self, r: Ref, i: usize, v: Ref) {
        let record = self.barrier && is_old(r) && is_young(v);
        let o = self.obj_mut(r);
        o.fields[i] = v;
        if record && !o.remembered {
            o.remembered = true; // remembered-set write barrier: old object now holds a young pointer
            self.rset.push(r);
        }
    }
    fn set_payload(&mut self, r: Ref, p: u64) {
        self.obj_mut(r).payload = p;
    }
    fn tick(&mut self, roots: &mut [Ref]) -> bool {
        if self.nursery.len() < self.nursery_cap {
            return false;
        }
        self.minor(roots, self.promote_age);
        if self.old.live >= self.old_trigger {
            self.full(roots);
        }
        true
    }
    fn collect(&mut self, roots: &mut [Ref]) {
        self.full(roots);
    }
    fn live_objects(&self) -> usize {
        self.nursery.len() + self.old.live
    }
    fn check_invariants(&self, roots: &[Ref]) -> Result<(), String> {
        let valid = |r: Ref| r == NIL || self.obj(r).is_some();
        if !roots.iter().all(|&r| valid(r)) {
            return Err("dangling root".into());
        }
        let all = self
            .nursery
            .iter()
            .chain(self.old.objs.iter().filter(|o| o.live));
        if all.clone().any(|o| !o.fields.iter().all(|&f| valid(f))) {
            return Err("dangling field".into());
        }
        // The generational invariant: every old object holding a young pointer is in the remembered set.
        for o in self.old.objs.iter().filter(|o| o.live) {
            if o.fields.iter().any(|&f| is_young(f)) && !o.remembered {
                return Err("old->young pointer missing from remembered set".into());
            }
        }
        if self
            .rset
            .iter()
            .any(|&r| !self.obj(r).is_some_and(|o| o.remembered))
        {
            return Err("remembered set entry without flag".into());
        }
        Ok(())
    }
    fn pauses(&self) -> &[usize] {
        &self.pauses
    }
    fn exact_after_tick(&self) -> bool {
        false // minor collections never look at old garbage
    }
}

// ---------- (e) reference counting + trial-deletion cycle collector ----------
struct RefCount {
    a: Arena,
    cycle_gc: bool,
    since: usize,
    trigger: usize,
    pauses: Vec<usize>,
}

impl RefCount {
    fn new(cycle_gc: bool) -> Self {
        RefCount {
            a: Arena::new(),
            cycle_gc,
            since: 0,
            trigger: 256,
            pauses: Vec::new(),
        }
    }
    fn inc(&mut self, r: Ref) {
        if r != NIL {
            self.a.objs[r as usize].rc += 1;
        }
    }
    /// Drop one reference; freeing cascades through children with an explicit worklist.
    fn dec(&mut self, r: Ref) {
        if r == NIL {
            return;
        }
        let mut freed = 0;
        let mut work = vec![r];
        while let Some(x) = work.pop() {
            let o = &mut self.a.objs[x as usize];
            o.rc -= 1;
            if o.rc == 0 {
                let fs = o.fields; // read before free() reuses field 0 as the free-list link
                self.a.free(x);
                freed += 1;
                work.extend(fs.into_iter().filter(|&f| f != NIL));
            }
        }
        if freed > 1 {
            self.pauses.push(freed); // a cascade is RC's only "pause"
        }
    }
}

impl Heap for RefCount {
    fn name(&self) -> &'static str {
        if self.cycle_gc {
            "refcount+cycle-gc"
        } else {
            "refcount (leaks cycles)"
        }
    }
    fn alloc(&mut self, tag: u8, payload: u64) -> Ref {
        self.since += 1;
        self.a.alloc(Obj::new(tag, payload))
    }
    fn view(&self, r: Ref) -> Option<View> {
        self.a.get(r).map(View::of)
    }
    fn set_field(&mut self, r: Ref, i: usize, v: Ref) {
        self.inc(v); // increment first so storing the value already there cannot free it
        let old = std::mem::replace(&mut self.a.objs[r as usize].fields[i], v);
        self.dec(old);
    }
    fn set_payload(&mut self, r: Ref, p: u64) {
        self.a.objs[r as usize].payload = p;
    }
    fn set_root(&mut self, roots: &mut [Ref], slot: usize, v: Ref) {
        self.inc(v);
        let old = std::mem::replace(&mut roots[slot], v);
        self.dec(old);
    }
    fn tick(&mut self, roots: &mut [Ref]) -> bool {
        if !self.cycle_gc || self.since < self.trigger {
            return false;
        }
        self.collect(roots);
        true
    }
    /// Trial deletion (as in CPython's gc): subtract every heap-internal reference from the counts. Objects
    /// left with a positive count are held from outside (roots); trace from them; the untouched rest is
    /// cyclic garbage (including whatever hangs off it).
    fn collect(&mut self, _roots: &mut [Ref]) {
        let n = self.a.objs.len();
        let mut gc: Vec<i64> = self
            .a
            .objs
            .iter()
            .map(|o| if o.live { o.rc as i64 } else { 0 })
            .collect();
        for o in self.a.objs.iter().filter(|o| o.live) {
            for &f in o.fields.iter().filter(|&&f| f != NIL) {
                gc[f as usize] -= 1;
            }
        }
        let mut reach = vec![false; n];
        let mut stack: Vec<usize> = (0..n)
            .filter(|&i| self.a.objs[i].live && gc[i] > 0)
            .collect();
        for &i in &stack {
            reach[i] = true;
        }
        while let Some(x) = stack.pop() {
            for f in self.a.objs[x].fields {
                if f != NIL && !reach[f as usize] {
                    reach[f as usize] = true;
                    stack.push(f as usize);
                }
            }
        }
        let garbage: Vec<usize> = (0..n)
            .filter(|&i| self.a.objs[i].live && !reach[i])
            .collect();
        for &g in &garbage {
            for f in self.a.objs[g].fields {
                if f != NIL && reach[f as usize] {
                    self.a.objs[f as usize].rc -= 1; // garbage's references to survivors disappear with it
                }
            }
        }
        for &g in &garbage {
            self.a.free(g as Ref);
        }
        self.pauses.push(n + self.a.live);
        self.since = 0;
        self.trigger = (self.a.live * 2).max(256);
    }
    fn live_objects(&self) -> usize {
        self.a.live
    }
    fn check_invariants(&self, roots: &[Ref]) -> Result<(), String> {
        self.a.check(roots)?;
        let mut indeg = vec![0u32; self.a.objs.len()];
        for &r in roots.iter().filter(|&&r| r != NIL) {
            indeg[r as usize] += 1;
        }
        for o in self.a.objs.iter().filter(|o| o.live) {
            for &f in o.fields.iter().filter(|&&f| f != NIL) {
                indeg[f as usize] += 1;
            }
        }
        for (i, o) in self.a.objs.iter().enumerate().filter(|(_, o)| o.live) {
            if o.rc != indeg[i] {
                return Err(format!(
                    "object {i}: rc {} but in-degree {}",
                    o.rc, indeg[i]
                ));
            }
        }
        Ok(())
    }
    fn pauses(&self) -> &[usize] {
        &self.pauses
    }
    fn leaks_cycles(&self) -> bool {
        !self.cycle_gc
    }
}

// ---------- the oracle: a model graph that never frees ----------
struct MObj {
    tag: u8,
    payload: u64,
    fields: [Option<usize>; NF],
}

struct Model {
    objs: Vec<MObj>,
    roots: Vec<Option<usize>>,
    gone: Vec<bool>, // garbage that some exact collection has already reclaimed
}

impl Model {
    fn new(nroots: usize) -> Model {
        Model {
            objs: Vec::new(),
            roots: vec![None; nroots],
            gone: Vec::new(),
        }
    }
    fn alloc(&mut self, tag: u8, payload: u64) -> usize {
        self.objs.push(MObj {
            tag,
            payload,
            fields: [None; NF],
        });
        self.gone.push(false);
        self.objs.len() - 1
    }
    fn reachable(&self) -> Vec<bool> {
        let mut seen = vec![false; self.objs.len()];
        let mut stack: Vec<usize> = self.roots.iter().flatten().copied().collect();
        while let Some(x) = stack.pop() {
            if !std::mem::replace(&mut seen[x], true) {
                stack.extend(self.objs[x].fields.iter().flatten());
            }
        }
        seen
    }
    /// What pure reference counting cannot free: garbage reachable from a garbage cycle. Peel garbage objects
    /// with no garbage referrers (what RC's cascade does); whatever cannot be peeled leaks.
    fn leaked_by_rc(&self, reach: &[bool]) -> usize {
        let in_g = |i: usize| !reach[i] && !self.gone[i];
        let mut indeg = vec![0usize; self.objs.len()];
        for i in (0..self.objs.len()).filter(|&i| in_g(i)) {
            for &t in self.objs[i].fields.iter().flatten().filter(|&&t| in_g(t)) {
                indeg[t] += 1;
            }
        }
        let mut queue: Vec<usize> = (0..self.objs.len())
            .filter(|&i| in_g(i) && indeg[i] == 0)
            .collect();
        let (mut peeled, total) = (0, (0..self.objs.len()).filter(|&i| in_g(i)).count());
        while let Some(x) = queue.pop() {
            peeled += 1;
            for &t in self.objs[x].fields.iter().flatten().filter(|&&t| in_g(t)) {
                indeg[t] -= 1;
                if indeg[t] == 0 {
                    queue.push(t);
                }
            }
        }
        total - peeled
    }
}

/// Walk heap and model in lock step from the roots; they must be isomorphic (same tags, payloads, shape,
/// sharing). Returns the number of reachable objects. A dangling handle is an error, not a panic.
fn check_graph(heap: &dyn Heap, roots: &[Ref], m: &Model) -> Result<usize, String> {
    let mut h2m: HashMap<Ref, usize> = HashMap::new();
    let mut m2h: HashMap<usize, Ref> = HashMap::new();
    let mut queue = VecDeque::new();
    for (s, &r) in roots.iter().enumerate() {
        match (r, m.roots[s]) {
            (NIL, None) => {}
            (NIL, Some(_)) | (_, None) => return Err(format!("root {s}: nil-ness differs")),
            (h, Some(mi)) => queue.push_back((h, mi)),
        }
    }
    while let Some((h, mi)) = queue.pop_front() {
        match (h2m.get(&h), m2h.get(&mi)) {
            (Some(&x), Some(&y)) if x == mi && y == h => continue,
            (None, None) => {}
            _ => return Err("sharing/cycle structure differs".into()),
        }
        h2m.insert(h, mi);
        m2h.insert(mi, h);
        let v = heap
            .view(h)
            .ok_or_else(|| format!("DANGLING reference {h:#x} (reachable object lost)"))?;
        let mo = &m.objs[mi];
        if v.tag != mo.tag || v.payload != mo.payload {
            return Err(format!(
                "contents differ at {h:#x}: heap payload {} vs model {}",
                v.payload, mo.payload
            ));
        }
        for i in 0..NF {
            match (v.fields[i], mo.fields[i]) {
                (NIL, None) => {}
                (NIL, Some(_)) | (_, None) => return Err("field nil-ness differs".into()),
                (hf, Some(mf)) => queue.push_back((hf, mf)),
            }
        }
    }
    Ok(h2m.len())
}

fn check(heap: &dyn Heap, roots: &[Ref], m: &mut Model, exact: bool) -> Result<usize, String> {
    let reachable = check_graph(heap, roots, m)?;
    heap.check_invariants(roots)?;
    let live = heap.live_objects();
    let reach = m.reachable();
    if heap.leaks_cycles() {
        let leaked = m.leaked_by_rc(&reach);
        if live != reachable + leaked {
            return Err(format!(
                "RC: live {live} != reachable {reachable} + cyclic garbage {leaked}"
            ));
        }
    } else if exact {
        if live != reachable {
            return Err(format!(
                "garbage not reclaimed: live {live} vs reachable {reachable}"
            ));
        }
        for (g, &r) in m.gone.iter_mut().zip(&reach) {
            *g |= !r;
        }
    } else if live < reachable {
        return Err("fewer live objects than reachable ones".into());
    }
    Ok(reachable)
}

// ---------- the random mutator ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        ((self.next() >> 11) % n as u64) as usize
    }
}

#[derive(Default, Debug)]
struct Report {
    cycles: usize,
    checks: usize,
    allocs: usize,
    max_live: usize,
    gc_time: Duration,
    total: Duration,
}

/// Pick an object by walking a random path from a random root, in lock step on heap and model.
fn pick(
    heap: &dyn Heap,
    roots: &[Ref],
    m: &Model,
    rng: &mut Rng,
) -> Result<Option<(Ref, usize)>, String> {
    let slot = rng.below(roots.len());
    let steps = rng.below(5);
    let (mut h, Some(mut mi)) = (roots[slot], m.roots[slot]) else {
        return if roots[slot] == NIL {
            Ok(None)
        } else {
            Err("root nil-ness differs".into())
        };
    };
    for _ in 0..steps {
        let i = rng.below(NF);
        let v = heap
            .view(h)
            .ok_or("DANGLING reference met by the mutator")?;
        match (v.fields[i], m.objs[mi].fields[i]) {
            (NIL, None) => break,
            (NIL, Some(_)) | (_, None) => return Err("field nil-ness differs".into()),
            (hf, Some(mf)) => (h, mi) = (hf, mf),
        }
    }
    let v = heap
        .view(h)
        .ok_or("DANGLING reference met by the mutator")?;
    if v.payload != m.objs[mi].payload {
        return Err("object contents corrupted".into());
    }
    Ok(Some((h, mi)))
}

fn drive(
    heap: &mut dyn Heap,
    seed: u64,
    ops: usize,
    nroots: usize,
    verify: bool,
) -> Result<Report, String> {
    let mut rng = Rng(seed);
    let mut m = Model::new(nroots);
    let mut roots = vec![NIL; nroots];
    let mut rep = Report::default();
    let start = Instant::now();
    let mut counter = 0u64;
    for step in 0..ops {
        match rng.below(100) {
            0..=29 => {
                counter += 1;
                let tag = rng.below(4) as u8;
                let slot = rng.below(nroots);
                let r = heap.alloc(tag, counter);
                let id = m.alloc(tag, counter);
                heap.set_root(&mut roots, slot, r);
                m.roots[slot] = Some(id);
                rep.allocs += 1;
            }
            30..=64 => {
                if let (Some(a), Some(b)) = (
                    pick(heap, &roots, &m, &mut rng)?,
                    pick(heap, &roots, &m, &mut rng)?,
                ) {
                    let i = rng.below(NF);
                    heap.set_field(a.0, i, b.0);
                    m.objs[a.1].fields[i] = Some(b.1);
                }
            }
            65..=74 => {
                if let Some(a) = pick(heap, &roots, &m, &mut rng)? {
                    let i = rng.below(NF);
                    heap.set_field(a.0, i, NIL);
                    m.objs[a.1].fields[i] = None;
                }
            }
            75..=82 => {
                let slot = rng.below(nroots);
                heap.set_root(&mut roots, slot, NIL);
                m.roots[slot] = None;
            }
            83..=90 => {
                if let Some(a) = pick(heap, &roots, &m, &mut rng)? {
                    let slot = rng.below(nroots);
                    heap.set_root(&mut roots, slot, a.0);
                    m.roots[slot] = Some(a.1);
                }
            }
            91..=94 => {
                if let Some(a) = pick(heap, &roots, &m, &mut rng)? {
                    counter += 1;
                    heap.set_payload(a.0, counter);
                    m.objs[a.1].payload = counter;
                }
            }
            _ => {
                // Build a 2-cycle (or a self-loop) on purpose: the case RC cannot reclaim.
                if let (Some(a), Some(b)) = (
                    pick(heap, &roots, &m, &mut rng)?,
                    pick(heap, &roots, &m, &mut rng)?,
                ) {
                    heap.set_field(a.0, 0, b.0);
                    m.objs[a.1].fields[0] = Some(b.1);
                    heap.set_field(b.0, 1, a.0);
                    m.objs[b.1].fields[1] = Some(a.1);
                }
            }
        }
        let t = Instant::now();
        let done = heap.tick(&mut roots);
        rep.gc_time += t.elapsed();
        rep.max_live = rep.max_live.max(heap.live_objects());
        if done {
            rep.cycles += 1;
            if verify {
                check(heap, &roots, &mut m, heap.exact_after_tick())?;
                rep.checks += 1;
                if !heap.exact_after_tick() && rep.cycles % 3 == 0 {
                    heap.collect(&mut roots); // a full collection settles floating garbage
                    check(heap, &roots, &mut m, true)?;
                    rep.checks += 1;
                }
            }
        } else if verify && heap.leaks_cycles() && step % 500 == 0 {
            check(heap, &roots, &mut m, false)?;
            rep.checks += 1;
        }
    }
    if verify {
        let t = Instant::now();
        heap.settle(&mut roots);
        if !heap.leaks_cycles() {
            heap.collect(&mut roots);
        }
        rep.gc_time += t.elapsed();
        check(heap, &roots, &mut m, true)?;
        rep.checks += 1;
    }
    rep.total = start.elapsed();
    Ok(rep)
}

// ---------- scripted scenarios ----------
fn both(h: &mut dyn Heap, m: &mut Model, tag: u8, payload: u64) -> (Ref, usize) {
    (h.alloc(tag, payload), m.alloc(tag, payload))
}
fn link(h: &mut dyn Heap, m: &mut Model, a: (Ref, usize), i: usize, b: (Ref, usize)) {
    h.set_field(a.0, i, b.0);
    m.objs[a.1].fields[i] = Some(b.1);
}

fn demo_mark_sweep() {
    let mut h = MarkSweep::new();
    let mut roots = [NIL; 4];
    let (a, b, c) = (h.alloc(0, 1), h.alloc(0, 2), h.alloc(0, 3));
    h.set_field(a, 0, b);
    h.set_field(b, 0, c);
    let (x, y, g) = (h.alloc(1, 10), h.alloc(1, 11), h.alloc(1, 12));
    h.set_field(x, 0, y);
    h.set_field(y, 0, x); // unreachable cycle: tracing does not care, RC would
    roots[0] = a;
    assert_eq!(h.live_objects(), 6);
    h.collect(&mut roots);
    assert_eq!(h.live_objects(), 3);
    assert!([x, y, g].iter().all(|&r| h.view(r).is_none()));
    let z = h.alloc(2, 99);
    assert!(
        [x, y, g].contains(&z),
        "allocation reuses a freed slot from the free list"
    );
    // A 100k-object list: a recursive marker would blow the stack, the explicit stack does not care.
    let mut head = NIL;
    for i in 0..100_000u64 {
        let n = h.alloc(3, 1000 + i);
        h.set_field(n, 0, head);
        head = n;
    }
    roots[1] = head;
    h.collect(&mut roots);
    assert_eq!(
        h.live_objects(),
        3 + 100_000,
        "a,b,c + the list; the unrooted z was freed"
    );
    println!(
        "(a) mark-sweep: cycle freed, slot reused, 100k-long list marked without recursion (live {})",
        h.live_objects()
    );
}

/// The classic lost-object schedule. Returns true if a reachable object was freed.
fn lost_object_schedule(barrier: bool) -> bool {
    let mut h = Incremental::new(barrier);
    let mut roots = [NIL; 3];
    let (a, b, c) = (h.alloc(0, 1), h.alloc(0, 2), h.alloc(0, 3));
    h.set_field(a, 0, b);
    h.set_field(b, 0, c);
    roots[0] = a;
    h.begin(&roots); // A grey
    assert_eq!(h.mark_step(1), 1); // A black, B grey, C still white
    roots[1] = h.view(b).unwrap().fields[0]; // mutator loads C into a (already scanned) register...
    h.set_field(a, 1, roots[1]); // ...stores it into the BLACK object A (the barrier fires here)...
    h.set_field(b, 0, NIL); // ...and removes the only other path.
    roots[1] = NIL;
    h.finish_mark(&roots);
    h.sweep_step(usize::MAX);
    h.view(c).is_none() // C is reachable via A.f1; was it freed?
}

fn demo_incremental() {
    assert!(
        lost_object_schedule(false),
        "without the barrier the reachable object C is freed"
    );
    assert!(
        !lost_object_schedule(true),
        "the insertion barrier shades C and saves it"
    );
    println!(
        "(b) incremental: barrier OFF loses reachable object C; barrier ON keeps it (Dijkstra insertion barrier)"
    );
}

fn demo_cheney() {
    let (mut h, mut m, mut roots) = (Cheney::new(), Model::new(2), [NIL; 2]);
    for i in 0..5 {
        both(&mut h, &mut m, 9, 500 + i); // garbage, allocated first so live data must slide down
    }
    let (a, b, c) = (
        both(&mut h, &mut m, 0, 1),
        both(&mut h, &mut m, 0, 2),
        both(&mut h, &mut m, 0, 3),
    );
    let shared = both(&mut h, &mut m, 1, 4);
    link(&mut h, &mut m, a, 0, b);
    link(&mut h, &mut m, b, 0, c);
    link(&mut h, &mut m, c, 0, a); // cycle
    link(&mut h, &mut m, a, 1, shared);
    link(&mut h, &mut m, b, 1, shared); // sharing
    roots[0] = a.0;
    m.roots[0] = Some(a.1);
    roots[1] = shared.0;
    m.roots[1] = Some(shared.1);
    let before = roots;
    assert_eq!(h.live_objects(), 9);
    h.collect(&mut roots);
    assert_ne!(before, roots, "addresses change when objects move");
    assert_eq!(
        h.live_objects(),
        4,
        "dead objects are not even touched; space is compacted"
    );
    assert_eq!(
        check_graph(&h, &roots, &m),
        Ok(4),
        "same graph: cycle and sharing preserved"
    );
    assert_eq!(
        roots,
        [0, 1],
        "roots are evacuated first, so to-space starts with them, densely"
    );
    println!(
        "(c) cheney: roots {before:?} -> {roots:?}, 9 objects -> 4 compacted, cycle + sharing intact"
    );
}

fn old_to_young(barrier: bool) -> bool {
    let mut g = Generational::new(barrier);
    let mut roots = [NIL; 2];
    roots[0] = g.alloc(0, 1);
    g.minor(&mut roots, 2);
    g.minor(&mut roots, 2); // second survival promotes (age 2)
    assert!(is_old(roots[0]));
    roots[1] = g.alloc(0, 2); // young object only reachable through the old one after the next lines
    g.set_field(roots[0], 0, roots[1]);
    assert_eq!(g.rset.len(), usize::from(barrier));
    roots[1] = NIL;
    g.minor(&mut roots, 2);
    let target = g.view(roots[0]).unwrap().fields[0];
    g.view(target).is_some_and(|v| v.payload == 2) // survived?
}

fn demo_generational() {
    assert!(
        old_to_young(true),
        "remembered set keeps the young child alive"
    );
    assert!(
        !old_to_young(false),
        "without the barrier the old->young pointer is lost"
    );
    println!(
        "(d) generational: old->young pointer survives a minor GC with the barrier, is lost without it"
    );
}

fn demo_refcount() {
    let mut h = RefCount::new(false);
    let mut roots = [NIL; 3];
    let (a, b, t) = (h.alloc(0, 1), h.alloc(0, 2), h.alloc(0, 3));
    h.set_root(&mut roots, 0, a);
    h.set_field(a, 0, b);
    h.set_field(b, 0, a); // cycle a <-> b
    h.set_field(b, 1, t); // tail hangs off the cycle
    let (c, d) = (h.alloc(0, 4), h.alloc(0, 5));
    h.set_root(&mut roots, 1, c);
    h.set_field(c, 0, d);
    h.set_root(&mut roots, 0, NIL);
    h.set_root(&mut roots, 1, NIL);
    assert!(
        h.view(c).is_none() && h.view(d).is_none(),
        "acyclic garbage is freed immediately"
    );
    assert_eq!(h.live_objects(), 3, "the cycle and its tail leak");
    assert!(h.check_invariants(&roots).is_ok());
    h.collect(&mut roots); // trial deletion
    assert_eq!(h.live_objects(), 0);
    println!(
        "(e) refcount: cascade freed c,d at once; cycle a<->b + tail leaked (3 objects) until trial deletion reclaimed them"
    );
}

// ---------- reporting ----------
fn pause_stats(p: &[usize]) -> (usize, usize, usize, f64) {
    if p.is_empty() {
        return (0, 0, 0, 0.0);
    }
    let mut s = p.to_vec();
    s.sort_unstable();
    (
        p.len(),
        s[s.len() / 2],
        *s.last().unwrap(),
        p.iter().sum::<usize>() as f64 / p.len() as f64,
    )
}

fn collectors() -> Vec<Box<dyn Heap>> {
    vec![
        Box::new(MarkSweep::new()),
        Box::new(Incremental::new(true)),
        Box::new(Cheney::new()),
        Box::new(Generational::new(true)),
        Box::new(RefCount::new(true)),
        Box::new(RefCount::new(false)),
    ]
}

fn main() {
    demo_mark_sweep();
    demo_incremental();
    demo_cheney();
    demo_generational();
    demo_refcount();

    println!(
        "\n== differential fuzz: random mutator vs reachability oracle (3 seeds x 30k ops) =="
    );
    for h in collectors().iter().map(|h| h.name()) {
        let mut cycles = 0;
        let mut checks = 0;
        for seed in [
            0x9E3779B97F4A7C15u64,
            0xD1B54A32D192ED03,
            0x2545F4914F6CDD1D,
        ] {
            let mut heap = collectors().into_iter().find(|c| c.name() == h).unwrap();
            let r = drive(heap.as_mut(), seed, 30_000, 16, true)
                .unwrap_or_else(|e| panic!("{h}: oracle failure: {e}"));
            cycles += r.cycles;
            checks += r.checks;
        }
        println!("  {h:<26} OK  gc cycles {cycles:>4}, oracle checks {checks:>4}");
    }

    println!("\n== barrier OFF under the random mutator (expected to be caught by the oracle) ==");
    // Random schedules rarely hit the exact window the scripted schedule above forces, so try several seeds.
    for barrier_off in [true, false] {
        let mut caught = 0;
        let mut name = "";
        for seed in 1..=8u64 {
            let mut heap: Box<dyn Heap> = if barrier_off {
                Box::new(Generational::new(false))
            } else {
                Box::new(Incremental::new(false))
            };
            name = heap.name();
            caught +=
                drive(heap.as_mut(), seed * 0x9E3779B97F4A7C15, 30_000, 16, true).is_err() as usize;
        }
        println!("  {name:<26} oracle caught corruption in {caught}/8 random runs");
        if barrier_off {
            assert!(caught > 0, "the remembered-set barrier is load-bearing");
        }
    }

    println!(
        "\n== pauses (objects scanned/swept per GC step) and throughput, 300k ops, 32 roots =="
    );
    println!(
        "  {:<26} {:>6} {:>7} {:>8} {:>8} {:>9} {:>8} {:>10}",
        "collector", "pauses", "median", "max", "mean", "gc ms", "total ms", "Mops/s"
    );
    for mut heap in collectors() {
        let name = heap.name();
        let rep = drive(heap.as_mut(), 7, 300_000, 32, false).expect("unverified run");
        let (n, med, max, mean) = pause_stats(heap.pauses());
        println!(
            "  {name:<26} {n:>6} {med:>7} {max:>8} {mean:>8.1} {:>9.1} {:>8.1} {:>10.2}   (peak live {})",
            rep.gc_time.as_secs_f64() * 1e3,
            rep.total.as_secs_f64() * 1e3,
            300_000.0 / rep.total.as_secs_f64() / 1e6,
            rep.max_live
        );
    }
    println!("(timings are machine-dependent and never asserted; pauses are in objects touched)");
}
