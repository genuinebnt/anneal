//! RECIPE 235 (systems): the io_uring DESIGN modelled in portable Rust: shared SQ/CQ rings, batching, linked SQEs, fixed buffers, async integration.
//! LEVEL: advanced
//! Use in: understanding completion-based I/O, tokio-uring/glommio-style runtimes, designing batched submission APIs.   Run: cargo run --release --example io_uring_model
#![allow(dead_code)]
//!
//! WHAT: epoll tells you "this fd is READY, now go do the syscall yourself" (readiness, 1+ syscalls per operation). io_uring is
//! COMPLETION based: you describe the operation (a Submission Queue Entry), the kernel performs it and posts the result (a Completion
//! Queue Entry). The two queues are ring buffers in memory shared between app and kernel, so many operations cost ONE `io_uring_enter`
//! (or zero with SQ polling). The real syscalls exist only on Linux, so here a fake "kernel" thread plays the other side of the same design.
//!
//! RINGS (same protocol as spsc_ring_buffer): each ring has a `head` (consumer) and `tail` (producer), monotonically increasing, slot = index & mask.
//!   SQ: app produces, kernel consumes.   CQ: kernel produces, app consumes.
//!   Producer: write slots, then `tail.store(.., Release)`: publishes ALL slots written before it, so a whole batch becomes visible
//!   atomically with one store. It reads `head` with Acquire to know slots were vacated.
//!   Consumer: `tail.load(Acquire)` makes the slot contents visible, read the slot, then `head.store(.., Release)` to hand it back.
//! `Producer`/`Consumer` are distinct non-Clone handles taking `&mut self`, so single-producer/single-consumer is enforced by the borrow checker.
//!
//! FEATURES MODELLED: `Sqe { opcode, fd, off, buf, len, user_data, flags }` / `Cqe { user_data, res }` (res < 0 is -errno);
//!   BATCHING (n SQEs, one enter); LINKS (`IOSQE_IO_LINK`: next SQE runs only if this one succeeds, else it completes with -ECANCELED);
//!   OUT-OF-ORDER completion (the kernel finishes independent ops in any order; `user_data` is how you match them up);
//!   FIXED/REGISTERED BUFFERS (register once, refer by index; saves per-op pinning/mapping, counted as `buffer_maps`).
//! ASYNC INTEGRATION: a tiny single-threaded executor (as in waker_executor). `read_at(..)` is an `async fn`: it pushes an SQE with a fresh `user_data`,
//!   stores a `Waker` in a `user_data -> Slot` table and returns Pending. The executor submits the batch, reaps the CQ, and for each CQE
//!   fills the slot and wakes the task. This is how tokio-uring and glommio map completions to futures.
//!   Caveat: completion I/O makes cancellation hard (drop a future while the kernel still owns its buffer) and buffers must be owned by the
//!   ring, which is why these runtimes pass buffers by value.
//! COMMON USES: tokio-uring, glommio, monoio, high-throughput storage engines, and servers on Linux 5.10+ that want fewer syscalls
//! CAVEAT: the io_uring side of the syscall comparison is SIMULATED by the fake kernel thread, and the epoll-style side is a FORMULA (one read per operation plus one epoll_wait per 128 ready operations), not a measured run. The Linux-only real probe and the epoll Poller in kqueue_epoll_raw were type-checked (cargo check --target x86_64-unknown-linux-gnu) but never executed on this macOS host.
//!   (also constrained by seccomp: many container runtimes disable io_uring by default).
//! Read: https://kernel.dk/io_uring.pdf , https://github.com/tokio-rs/io-uring , https://github.com/axboe/liburing
use std::cell::{RefCell, UnsafeCell};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ the shared rings (SPSC, as spsc_ring_buffer)
#[repr(align(64))]
struct CachePadded<T>(T);

struct Shared<T> {
    buf: Box<[UnsafeCell<T>]>,
    mask: usize,
    head: CachePadded<AtomicUsize>, // advanced only by the consumer
    tail: CachePadded<AtomicUsize>, // advanced only by the producer
}
// SAFETY: a slot is written by the single Producer before it publishes `tail` (Release) and read by the single Consumer only after
// observing that `tail` (Acquire); the slot is not rewritten until the consumer publishes `head` (Release) and the producer sees it (Acquire).
unsafe impl<T: Send> Send for Shared<T> {}
// SAFETY: as above; all cross-thread slot access is ordered by the head/tail atomics.
unsafe impl<T: Send> Sync for Shared<T> {}

struct Producer<T>(Arc<Shared<T>>);
struct Consumer<T>(Arc<Shared<T>>);

fn ring<T: Copy + Default>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let cap = capacity.next_power_of_two();
    let sh = Arc::new(Shared {
        buf: (0..cap).map(|_| UnsafeCell::new(T::default())).collect(),
        mask: cap - 1,
        head: CachePadded(AtomicUsize::new(0)),
        tail: CachePadded(AtomicUsize::new(0)),
    });
    (Producer(sh.clone()), Consumer(sh))
}

impl<T: Copy> Producer<T> {
    /// Writes as many of `items` as fit, then publishes them ALL with a single Release store. Returns how many were accepted.
    fn push_batch(&mut self, items: &[T]) -> usize {
        let s = &*self.0;
        let tail = s.tail.0.load(Ordering::Relaxed); // we are the only writer of tail
        let head = s.head.0.load(Ordering::Acquire); // pairs with consumer's Release: slots up to head are vacated
        let free = s.buf.len() - tail.wrapping_sub(head);
        let n = free.min(items.len());
        for (i, it) in items[..n].iter().enumerate() {
            // SAFETY: slots in [tail, tail+n) are vacated (checked via head) and not yet visible to the consumer; we are the only producer.
            unsafe { *s.buf[tail.wrapping_add(i) & s.mask].get() = *it };
        }
        s.tail.0.store(tail.wrapping_add(n), Ordering::Release); // publish the whole batch at once
        n
    }
}

impl<T: Copy> Consumer<T> {
    fn pop(&mut self) -> Option<T> {
        let s = &*self.0;
        let head = s.head.0.load(Ordering::Relaxed); // we are the only writer of head
        if head == s.tail.0.load(Ordering::Acquire) {
            return None; // Acquire pairs with producer's Release: contents below tail are visible
        }
        // SAFETY: head != tail, so this slot was published by the producer and is not rewritten until we advance head.
        let v = unsafe { *s.buf[head & s.mask].get() };
        s.head.0.store(head.wrapping_add(1), Ordering::Release); // hand the slot back
        Some(v)
    }
}

// ------------------------------------------------------------------ queue entries
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Op {
    #[default]
    Nop,
    Read,
    Write,
}

pub const IOSQE_IO_LINK: u8 = 1 << 2; // same bit as the real flag
pub const SQE_FIXED_BUF: u8 = 1 << 7; // model-only: the real API uses READ_FIXED/WRITE_FIXED opcodes
const EBADF: i32 = 9;
const ECANCELED: i32 = 125;

#[derive(Clone, Copy, Debug, Default)]
struct Sqe {
    opcode: Op,
    fd: i32,
    off: u64,
    buf: u32, // index into the buffer table
    len: u32,
    user_data: u64,
    flags: u8,
}

#[derive(Clone, Copy, Debug, Default)]
struct Cqe {
    user_data: u64,
    res: i32, // bytes transferred, or -errno
}

// ------------------------------------------------------------------ the fake kernel
#[derive(Default)]
struct KStats {
    buffer_maps: AtomicU64,
    executed: AtomicU64,
    cancelled: AtomicU64,
}

enum Msg {
    Enter,
    Shutdown,
}

type Files = Arc<Mutex<HashMap<i32, Vec<u8>>>>;
type Buffers = Arc<Vec<Mutex<Vec<u8>>>>;

fn latency(user_data: u64) -> u64 {
    1 + (user_data.wrapping_mul(2654435761) >> 7) % 8 // deterministic pseudo "device time"
}

fn execute(s: &Sqe, files: &Files, bufs: &Buffers, st: &KStats) -> i32 {
    st.executed.fetch_add(1, Ordering::Relaxed);
    if s.flags & SQE_FIXED_BUF == 0 && s.opcode != Op::Nop {
        st.buffer_maps.fetch_add(1, Ordering::Relaxed); // unregistered buffer: pin/map pages on every operation
    }
    match s.opcode {
        Op::Nop => 0,
        Op::Read => {
            let files = files.lock().unwrap();
            let Some(f) = files.get(&s.fd) else {
                return -EBADF;
            };
            let mut b = bufs[s.buf as usize].lock().unwrap();
            let off = (s.off as usize).min(f.len());
            let n = (s.len as usize).min(f.len() - off).min(b.len());
            b[..n].copy_from_slice(&f[off..off + n]);
            n as i32
        }
        Op::Write => {
            let mut files = files.lock().unwrap();
            let Some(f) = files.get_mut(&s.fd) else {
                return -EBADF;
            };
            let b = bufs[s.buf as usize].lock().unwrap();
            let (off, n) = (s.off as usize, (s.len as usize).min(b.len()));
            if f.len() < off + n {
                f.resize(off + n, 0);
            }
            f[off..off + n].copy_from_slice(&b[..n]);
            n as i32
        }
    }
}

/// Processes one batch: chains run in order, different chains finish in "device time" order, so completions arrive out of order.
fn kernel_batch(batch: &[Sqe], files: &Files, bufs: &Buffers, st: &KStats) -> Vec<Cqe> {
    let mut done: Vec<(u64, Cqe)> = Vec::with_capacity(batch.len());
    let (mut clock, mut broken) = (0u64, false);
    for s in batch {
        clock += latency(s.user_data); // within a chain times accumulate; it resets when a chain ends
        let linked = s.flags & IOSQE_IO_LINK != 0;
        let res = if broken {
            st.cancelled.fetch_add(1, Ordering::Relaxed);
            -ECANCELED
        } else {
            execute(s, files, bufs, st)
        };
        let short = matches!(s.opcode, Op::Read | Op::Write) && res >= 0 && (res as u32) < s.len;
        if linked {
            broken = broken || res < 0 || short; // failure (or short transfer) poisons the rest of the chain
        } else {
            broken = false;
        }
        done.push((
            clock,
            Cqe {
                user_data: s.user_data,
                res,
            },
        ));
        if !linked {
            clock = 0;
        }
    }
    done.sort_by_key(|d| d.0); // stable: ties keep submission order
    done.into_iter().map(|d| d.1).collect()
}

fn kernel_thread(
    mut sq: Consumer<Sqe>,
    mut cq: Producer<Cqe>,
    files: Files,
    bufs: Buffers,
    rx: Receiver<Msg>,
    done: Sender<()>,
    st: Arc<KStats>,
) {
    while let Ok(Msg::Enter) = rx.recv() {
        let mut batch = Vec::new();
        while let Some(s) = sq.pop() {
            batch.push(s);
        }
        let cqes = kernel_batch(&batch, &files, &bufs, &st);
        let mut sent = 0;
        let t = Instant::now();
        while sent < cqes.len() {
            sent += cq.push_batch(&cqes[sent..]);
            if sent < cqes.len() {
                assert!(
                    t.elapsed() < Duration::from_secs(5),
                    "CQ overflow: app is not reaping"
                );
                thread::yield_now();
            }
        }
        let _ = done.send(()); // wake an app blocked in "enter(GETEVENTS)"
    }
}

// ------------------------------------------------------------------ app side: the uring handle
struct Slot {
    res: Option<i32>,
    waker: Option<Waker>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Syscalls {
    register: u64,
    submit_enters: u64,
    wait_enters: u64,
    sqes: u64,
}
impl Syscalls {
    fn total(&self) -> u64 {
        self.register + self.submit_enters + self.wait_enters
    }
}

struct Inner {
    sq: Producer<Sqe>,
    cq: Consumer<Cqe>,
    pending: Vec<Sqe>,
    slots: HashMap<u64, Slot>,
    next_ud: u64,
    inflight: usize,
    tx: Sender<Msg>,
    done_rx: Receiver<()>,
    sys: Syscalls,
    kernel: Option<JoinHandle<()>>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Shutdown);
        if let Some(h) = self.kernel.take() {
            let _ = h.join();
        }
    }
}

#[derive(Clone)]
struct Uring {
    inner: Rc<RefCell<Inner>>,
    bufs: Buffers,
    kstats: Arc<KStats>,
}

impl Uring {
    fn new(sq_entries: usize, nbufs: usize, bufsize: usize, files: HashMap<i32, Vec<u8>>) -> Uring {
        let (sq_p, sq_c) = ring::<Sqe>(sq_entries);
        let (cq_p, cq_c) = ring::<Cqe>(sq_entries * 2); // like the real default: CQ is twice the SQ
        let (tx, rx) = channel();
        let (done_tx, done_rx) = channel();
        let bufs: Buffers = Arc::new((0..nbufs).map(|_| Mutex::new(vec![0u8; bufsize])).collect());
        let (files, kstats) = (Arc::new(Mutex::new(files)), Arc::new(KStats::default()));
        let h = {
            let (b, st) = (bufs.clone(), kstats.clone());
            thread::spawn(move || kernel_thread(sq_c, cq_p, files, b, rx, done_tx, st))
        };
        let inner = Inner {
            sq: sq_p,
            cq: cq_c,
            pending: Vec::new(),
            slots: HashMap::new(),
            next_ud: 1,
            inflight: 0,
            tx,
            done_rx,
            sys: Syscalls {
                register: 1,
                ..Default::default()
            }, // io_uring_register(BUFFERS) once
            kernel: Some(h),
        };
        Uring {
            inner: Rc::new(RefCell::new(inner)),
            bufs,
            kstats,
        }
    }

    /// Queue SQEs locally (no syscall). Returns a future for all their results, in order.
    fn submit(&self, mut sqes: Vec<Sqe>) -> Completion {
        let mut i = self.inner.borrow_mut();
        let mut ids = Vec::new();
        for s in sqes.iter_mut() {
            s.user_data = i.next_ud;
            i.next_ud += 1;
            ids.push(s.user_data);
            i.slots.insert(
                s.user_data,
                Slot {
                    res: None,
                    waker: None,
                },
            );
        }
        i.inflight += sqes.len();
        i.pending.extend(sqes);
        Completion {
            ring: self.clone(),
            ids,
        }
    }

    /// Publish the pending batch into the SQ (one Release store) and do ONE "io_uring_enter".
    fn flush(&self) {
        let mut i = self.inner.borrow_mut();
        if i.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut i.pending);
        let n = i.sq.push_batch(&pending);
        i.pending = pending[n..].to_vec(); // SQ was full: the rest goes next time
        if n > 0 {
            i.sys.submit_enters += 1;
            i.sys.sqes += n as u64;
            i.tx.send(Msg::Enter).unwrap();
        }
    }

    /// Drain the CQ without a syscall; fill slots and wake tasks. Returns the number of CQEs.
    fn reap(&self) -> usize {
        let mut i = self.inner.borrow_mut();
        let mut n = 0;
        while let Some(c) = i.cq.pop() {
            let slot = i.slots.get_mut(&c.user_data).expect("unknown user_data");
            slot.res = Some(c.res);
            if let Some(w) = slot.waker.take() {
                w.wake();
            }
            i.inflight -= 1;
            n += 1;
        }
        n
    }

    /// Block (with a timeout) until the kernel posts something: models `io_uring_enter(GETEVENTS)`.
    fn wait(&self) {
        if self.reap() > 0 {
            return;
        }
        {
            let mut i = self.inner.borrow_mut();
            i.sys.wait_enters += 1;
            i.done_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("kernel never completed");
        }
        self.reap();
    }

    fn sys(&self) -> Syscalls {
        self.inner.borrow().sys
    }

    fn sqe(&self, opcode: Op, fd: i32, off: u64, buf: u32, len: u32, flags: u8) -> Sqe {
        Sqe {
            opcode,
            fd,
            off,
            buf,
            len,
            user_data: 0,
            flags,
        }
    }

    async fn read_at(
        &self,
        fd: i32,
        off: u64,
        len: u32,
        buf: u32,
        fixed: bool,
    ) -> std::io::Result<Vec<u8>> {
        let f = if fixed { SQE_FIXED_BUF } else { 0 };
        let res = self
            .submit(vec![self.sqe(Op::Read, fd, off, buf, len, f)])
            .await[0];
        if res < 0 {
            return Err(std::io::Error::from_raw_os_error(-res));
        }
        Ok(self.bufs[buf as usize].lock().unwrap()[..res as usize].to_vec())
    }

    /// Submit a linked chain; every step except the last gets IOSQE_IO_LINK. Returns each step's `res`.
    async fn chain(&self, mut steps: Vec<Sqe>) -> Vec<i32> {
        let last = steps.len() - 1;
        for s in &mut steps[..last] {
            s.flags |= IOSQE_IO_LINK;
        }
        self.submit(steps).await
    }
}

struct Completion {
    ring: Uring,
    ids: Vec<u64>,
}
impl Future for Completion {
    type Output = Vec<i32>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Vec<i32>> {
        let mut i = self.ring.inner.borrow_mut();
        if self.ids.iter().all(|id| i.slots[id].res.is_some()) {
            return Poll::Ready(
                self.ids
                    .iter()
                    .map(|id| i.slots.remove(id).unwrap().res.unwrap())
                    .collect(),
            );
        }
        for id in &self.ids {
            i.slots.get_mut(id).unwrap().waker = Some(cx.waker().clone()); // user_data -> Waker
        }
        Poll::Pending
    }
}

// ------------------------------------------------------------------ a tiny executor (as waker_executor)
struct TaskWaker {
    id: usize,
    queue: Arc<Mutex<VecDeque<usize>>>,
}
impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.queue.lock().unwrap().push_back(self.id);
    }
}

type Task = Pin<Box<dyn Future<Output = ()>>>;

fn run_all(ring: &Uring, tasks: Vec<Task>) {
    let queue = Arc::new(Mutex::new((0..tasks.len()).collect::<VecDeque<_>>()));
    let wakers: Vec<Waker> = (0..tasks.len())
        .map(|id| {
            Waker::from(Arc::new(TaskWaker {
                id,
                queue: queue.clone(),
            }))
        })
        .collect();
    let mut tasks: Vec<Option<Task>> = tasks.into_iter().map(Some).collect();
    let (mut remaining, deadline) = (tasks.len(), Instant::now() + Duration::from_secs(15));
    while remaining > 0 {
        assert!(Instant::now() < deadline, "executor timed out");
        loop {
            let next = queue.lock().unwrap().pop_front();
            let Some(id) = next else { break };
            if let Some(t) = tasks[id].as_mut() {
                if t.as_mut()
                    .poll(&mut Context::from_waker(&wakers[id]))
                    .is_ready()
                {
                    tasks[id] = None;
                    remaining -= 1;
                }
            }
        }
        ring.flush(); // everything the tasks queued goes out in ONE batch
        if remaining > 0 && queue.lock().unwrap().is_empty() {
            ring.wait();
        }
    }
}

// ------------------------------------------------------------------ demos
fn file_byte(fd: i32, j: usize) -> u8 {
    (fd as usize * 31 + j) as u8
}
fn make_files() -> HashMap<i32, Vec<u8>> {
    (3..=6)
        .map(|fd| (fd, (0..4096).map(|j| file_byte(fd, j)).collect()))
        .collect()
}

fn demo_rings() {
    let (mut p, mut c) = ring::<u32>(4);
    assert_eq!(
        p.push_batch(&[1, 2, 3, 4, 5, 6]),
        4,
        "only 4 slots: the rest is refused, not overwritten"
    );
    assert_eq!((c.pop(), c.pop()), (Some(1), Some(2)));
    assert_eq!(
        p.push_batch(&[7, 8, 9]),
        2,
        "freed slots are reusable (wraparound)"
    );
    let rest: Vec<u32> = std::iter::from_fn(|| c.pop()).collect();
    assert_eq!(rest, [3, 4, 7, 8]);
    // two threads, a million-ish transfers, order preserved
    let (mut p, mut c) = ring::<u64>(64);
    let t = thread::spawn(move || {
        let mut sent = 0u64;
        while sent < 200_000 {
            sent += p.push_batch(&[sent, sent + 1, sent + 2, sent + 3]) as u64; // partial acceptance is fine: resume at `sent`
        }
    });
    let (mut expect, deadline) = (0u64, Instant::now() + Duration::from_secs(10));
    while expect < 200_000 {
        assert!(Instant::now() < deadline);
        match c.pop() {
            Some(v) => {
                assert_eq!(v, expect);
                expect += 1;
            }
            None => thread::yield_now(),
        }
    }
    t.join().unwrap();
    println!("rings: wraparound + 200k cross-thread items in order");
}

fn demo_out_of_order() {
    let ring = Uring::new(64, 16, 64, make_files());
    let order = Rc::new(RefCell::new(Vec::new()));
    let tasks: Vec<Task> = (0..16u32)
        .map(|k| {
            let (ring, order) = (ring.clone(), order.clone());
            Box::pin(async move {
                let data = ring
                    .read_at(3 + (k % 4) as i32, 8, 16, k, true)
                    .await
                    .unwrap();
                assert_eq!(data[0], file_byte(3 + (k % 4) as i32, 8));
                order.borrow_mut().push(k);
            }) as Task
        })
        .collect();
    run_all(&ring, tasks);
    let order = order.borrow().clone();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        (0..16).collect::<Vec<_>>(),
        "every op completed exactly once"
    );
    assert_ne!(order, sorted, "completions arrive out of submission order");
    let s = ring.sys();
    assert_eq!(
        (s.submit_enters, s.sqes),
        (1, 16),
        "16 SQEs, ONE submit syscall"
    );
    println!(
        "out-of-order: 16 reads in {} submit syscall; completion order {order:?}",
        s.submit_enters
    );
}

fn demo_links() {
    let ring = Uring::new(64, 4, 256, make_files());
    // chain 1: read file 3 -> buffer 0, write buffer 0 -> file 5 (a "copy"), both succeed
    // chain 2: read ok, read bad fd (-EBADF), write (cancelled), nop (cancelled)
    // chain 3 (unlinked sibling submitted in the same batch) still runs
    let r = Rc::new(RefCell::new((Vec::new(), Vec::new(), 0)));
    let (a, b, c) = (ring.clone(), ring.clone(), ring.clone());
    let (r1, r2, r3) = (r.clone(), r.clone(), r.clone());
    let tasks: Vec<Task> = vec![
        Box::pin(async move {
            let steps = vec![
                a.sqe(Op::Read, 3, 0, 0, 128, SQE_FIXED_BUF),
                a.sqe(Op::Write, 5, 0, 0, 128, SQE_FIXED_BUF),
            ];
            r1.borrow_mut().0 = a.chain(steps).await;
        }),
        Box::pin(async move {
            let steps = vec![
                b.sqe(Op::Read, 4, 0, 1, 64, SQE_FIXED_BUF),
                b.sqe(Op::Read, 99, 0, 1, 64, SQE_FIXED_BUF),
                b.sqe(Op::Write, 6, 0, 1, 64, SQE_FIXED_BUF),
                b.sqe(Op::Nop, 0, 0, 0, 0, 0),
            ];
            r2.borrow_mut().1 = b.chain(steps).await;
        }),
        Box::pin(async move {
            r3.borrow_mut().2 = c.read_at(6, 0, 32, 2, true).await.unwrap().len();
        }),
    ];
    run_all(&ring, tasks);
    let (c1, c2, c3) = r.borrow().clone();
    assert_eq!(c1, [128, 128]);
    assert_eq!(
        c2,
        [64, -EBADF, -ECANCELED, -ECANCELED],
        "a failure cancels the rest of ITS chain only"
    );
    assert_eq!(c3, 32);
    assert_eq!(ring.kstats.cancelled.load(Ordering::Relaxed), 2);
    let copied = ring.bufs[0].lock().unwrap()[..128].to_vec();
    assert_eq!(
        copied,
        (0..128).map(|j| file_byte(3, j)).collect::<Vec<_>>()
    );
    // the cancelled write to fd 6 never happened: re-read fd 6 and compare with the pristine content
    let ring2 = ring.clone();
    let ok = Rc::new(RefCell::new(false));
    let ok2 = ok.clone();
    run_all(
        &ring,
        vec![Box::pin(async move {
            let d = ring2.read_at(6, 0, 64, 3, true).await.unwrap();
            *ok2.borrow_mut() = d == (0..64).map(|j| file_byte(6, j)).collect::<Vec<_>>();
        })],
    );
    assert!(
        *ok.borrow(),
        "cancelled write must not have modified the file"
    );
    println!("links: chain1 {c1:?}  chain2 {c2:?}  (independent op unaffected: {c3} bytes)");
}

fn workload(fixed: bool, total: usize) -> (Syscalls, u64) {
    const WORKERS: usize = 128; // in-flight window <= CQ capacity (512), so the kernel never blocks on a full CQ
    let ring = Uring::new(256, WORKERS, 64, make_files());
    let tasks: Vec<Task> = (0..WORKERS)
        .map(|w| {
            let ring = ring.clone();
            let n = total / WORKERS + usize::from(w < total % WORKERS);
            Box::pin(async move {
                for k in 0..n {
                    let fd = 3 + ((w + k) % 4) as i32;
                    let off = (k * 64 % 4000) as u64;
                    let d = ring.read_at(fd, off, 16, w as u32, fixed).await.unwrap();
                    assert_eq!(d[0], file_byte(fd, off as usize));
                }
            }) as Task
        })
        .collect();
    run_all(&ring, tasks);
    (ring.sys(), ring.kstats.buffer_maps.load(Ordering::Relaxed))
}

/// Readiness (epoll) model for `ops` reads: each ready op costs one read(2); each epoll_wait returns at most `ready_batch` ready fds.
fn readiness_syscalls(ops: u64, ready_batch: u64) -> u64 {
    ops + ops.div_ceil(ready_batch)
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Default)]
struct IoUringParams {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: [u32; 10], // struct io_sqring_offsets (40 bytes)
    cq_off: [u32; 10], // struct io_cqring_offsets (40 bytes)
}

#[cfg(target_os = "linux")]
fn real_probe() {
    let mut p = IoUringParams::default();
    // SAFETY: io_uring_setup(entries, *params) only reads/writes the params struct, which is a correctly sized repr(C) (120 bytes)
    // zeroed value that lives for the whole call.
    let fd = unsafe { libc::syscall(libc::SYS_io_uring_setup, 4u32, &mut p as *mut IoUringParams) };
    if fd < 0 {
        let e = std::io::Error::last_os_error();
        match e.raw_os_error() {
            Some(libc::ENOSYS) => println!("real io_uring: kernel too old (ENOSYS)"),
            Some(libc::EPERM) => println!("real io_uring: blocked by seccomp/sysctl (EPERM)"),
            _ => println!("real io_uring: setup failed: {e}"),
        }
        return;
    }
    println!(
        "real io_uring: ring fd {fd}, sq_entries={}, cq_entries={}, features={:#x}",
        p.sq_entries, p.cq_entries, p.features
    );
    assert_eq!(std::mem::size_of::<IoUringParams>(), 120);
    // SAFETY: `fd` is a valid descriptor we just created and own; closed exactly once.
    unsafe { libc::close(fd as i32) };
}

fn main() {
    demo_rings();
    demo_out_of_order();
    demo_links();

    let ops = 10_000u64;
    let (sys, maps_fixed) = workload(true, ops as usize);
    let epoll = readiness_syscalls(ops, 128);
    assert_eq!(sys.sqes, ops);
    assert!(
        sys.total() * 10 < epoll,
        "batched submission should cut syscalls >10x: {sys:?} vs {epoll}"
    );
    assert_eq!(maps_fixed, 0, "registered buffers: no per-op mapping");
    println!(
        "10k reads: io_uring model = {} syscalls ({} submit + {} wait + {} register), readiness model = {} ({:.0}x more)",
        sys.total(),
        sys.submit_enters,
        sys.wait_enters,
        sys.register,
        epoll,
        epoll as f64 / sys.total() as f64
    );
    let (_, maps_plain) = workload(false, 1000);
    assert_eq!(maps_plain, 1000);
    println!(
        "buffer maps: registered={maps_fixed} per 10k ops, unregistered={maps_plain} per 1k ops"
    );

    #[cfg(target_os = "linux")]
    real_probe();
    #[cfg(not(target_os = "linux"))]
    println!("real io_uring probe: skipped (Linux only)");
    println!("r235 ok");
}
