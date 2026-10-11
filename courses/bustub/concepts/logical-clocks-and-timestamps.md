---
title: Logical clocks: counting events instead of reading the time
summary: Why a replacer, a lock manager and a transaction system all number events with a counter rather than the wall clock, and the clocks Rust gives you when you do need real time.
minutes: 6
---
LRU-K needs to say "this access happened before that one". The obvious tool is the system clock. It is the wrong one.

## Three reasons not to use wall-clock time for ordering

1. **Ties.** Two accesses a few nanoseconds apart can read the same clock value on a coarse clock, and then "which was first?" has no answer.
2. **It can go backwards.** `SystemTime` follows the real-time clock, which an NTP correction or an administrator can step back. `Instant` is monotonic (it never goes back) but its resolution and origin are unspecified.
3. **Tests become non-deterministic.** A test that asserts which frame is evicted must not depend on how fast the machine ran.

## A logical clock

A **logical clock** is a counter you advance yourself, one tick per event that matters:

```rust
pub struct LruKReplacer {
    current_timestamp: usize,       // advances by one on every recorded access
    /* ... */
}

pub fn record_access(&mut self, frame: FrameId) {
    let now = self.current_timestamp;
    self.current_timestamp += 1;
    /* node.record(now) */
}
```

Properties: **total order** (no two events share a value), **deterministic** (replaying the same calls gives the same stamps), **free** (an increment), and **meaningless as a duration** (timestamp 100 is not "100 ms"). For eviction order that is exactly what you want: only the *order* matters.

Leslie Lamport's 1978 paper generalised this to distributed systems (each process keeps a counter and takes the maximum of its own and any received one), and **transaction timestamps** in module 4's MVCC are the same idea: a monotonically increasing number that says who is older.

```svg
caption: Two reasons to keep your own counter. Above, inside one program: a coarse wall clock gives the first two events the same reading, so "which came first?" has no answer; a counter you advance gives every event its own number. Below, between two processes (Lamport): a received message makes the receiver jump to the larger of its own clock and the sender's, plus one, so a cause always has a smaller stamp than its effect. (Animated: the stamps arrive in order.)
<svg viewBox="0 0 760 350" role="img" aria-label="Events stamped by a counter while a wall clock ties, and two processes exchanging a stamped message so that the receiver takes the maximum">
<style>
@keyframes lc-1{0%,8%{opacity:.15}16%,100%{opacity:1}}
@keyframes lc-2{0%,24%{opacity:.15}32%,100%{opacity:1}}
@keyframes lc-3{0%,40%{opacity:.15}48%,100%{opacity:1}}
@keyframes lc-4{0%,56%{opacity:.15}64%,100%{opacity:1}}
@keyframes lc-m{0%,70%{opacity:.15}78%,92%{opacity:1}100%{opacity:.15}}
.lc-1{animation:lc-1 12s ease-in-out infinite}.lc-2{animation:lc-2 12s ease-in-out infinite}.lc-3{animation:lc-3 12s ease-in-out infinite}.lc-4{animation:lc-4 12s ease-in-out infinite}.lc-m{animation:lc-m 12s ease-in-out infinite}
</style>
<defs><marker id="lc-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--acc)"/></marker></defs>
<text class="dim sm" x="30" y="24">INSIDE ONE PROGRAM</text>
<path class="ln" d="M40 70 H720"/>
<g class="lc-1"><circle class="live" cx="110" cy="70" r="14"/><text class="mid fg" x="110" y="75">A</text><text class="mid t-g sm" x="110" y="104">counter 0</text><text class="mid t-r sm" x="110" y="122">wall 12:00:00.001</text></g>
<g class="lc-2"><circle class="live" cx="250" cy="70" r="14"/><text class="mid fg" x="250" y="75">B</text><text class="mid t-g sm" x="250" y="104">counter 1</text><text class="mid t-r sm" x="250" y="122">wall 12:00:00.001</text></g>
<g class="lc-3"><circle class="live" cx="390" cy="70" r="14"/><text class="mid fg" x="390" y="75">A</text><text class="mid t-g sm" x="390" y="104">counter 2</text><text class="mid dim sm" x="390" y="122">wall 12:00:00.002</text></g>
<g class="lc-4"><circle class="live" cx="530" cy="70" r="14"/><text class="mid fg" x="530" y="75">C</text><text class="mid t-g sm" x="530" y="104">counter 3</text><text class="mid dim sm" x="530" y="122">wall 12:00:00.002</text></g>
<text class="dim sm" x="30" y="150">A and B read the same wall clock: a tie. The counter never repeats and never goes back.</text>
<text class="dim sm" x="30" y="190">BETWEEN TWO PROCESSES</text>
<path class="ln" d="M40 230 H720"/><path class="ln" d="M40 300 H720"/>
<text class="dim sm" x="30" y="222">P</text><text class="dim sm" x="30" y="292">Q</text>
<circle class="blue" cx="130" cy="230" r="11"/><text class="mid fg sm" x="130" y="234">4</text>
<circle class="blue" cx="290" cy="230" r="11"/><text class="mid fg sm" x="290" y="234">5</text><text class="end dim sm" x="276" y="256">send, stamp 5</text>
<circle class="blue" cx="130" cy="300" r="11"/><text class="mid fg sm" x="130" y="304">1</text>
<circle class="blue" cx="210" cy="300" r="11"/><text class="mid fg sm" x="210" y="304">2</text>
<g class="lc-m"><path class="ln-w" d="M290 242 L420 288" marker-end="url(#lc-a)"/><circle class="hot" cx="430" cy="300" r="13"/><text class="mid fg sm" x="430" y="304">6</text>
<text class="t-a sm" x="460" y="296">receive: max(own 2, stamp 5) + 1 = 6</text></g>
<text class="dim sm" x="30" y="338">A stamp is an order, not a duration: 100 is not "100 ms". A cause is always stamped lower than its effect.</text>
</svg>
```

## When you do need real time

| need | use | notes |
|---|---|---|
| measure how long something took | `std::time::Instant::now()` and `.elapsed()` | monotonic; the tests' timing assertions use it |
| a timestamp to store or show | `std::time::SystemTime` | can jump; convert with `duration_since(UNIX_EPOCH)` |
| sleep or timeout | `thread::sleep`, `Condvar::wait_timeout`, `recv_timeout` | |
| a counter shared by threads | `AtomicU64::fetch_add(1, Relaxed)` | a logical clock for concurrent code; `Relaxed` is enough when only uniqueness matters |

> [!PORT] C++
> `std::chrono::steady_clock` is Rust's `Instant`; `system_clock` is `SystemTime`. Using `system_clock::now()` to order events is a classic bug in ported code: use a counter.

## Invariants: what must always be true

- **A clock only moves forward.** It never repeats a value and never goes back, so two events in one program never share a stamp.
- **Stamp order is event order.** Replaying the same calls gives the same stamps, which is what makes a test about "who is evicted first" deterministic.
- **On receive, the clock becomes `max(own, received) + 1`.** Afterwards a cause always carries a smaller stamp than its effect, across processes too.
- **A stamp is not a duration.** Differences between stamps count events, not milliseconds; never use them to time anything.

## A caveat

A counter in a `usize` wraps after 2<sup>64</sup> increments on a 64-bit machine, which at one billion events per second takes about 585 years. On a 32-bit `usize` it takes about 4 seconds at that rate: the reference replacer's counter is a `usize` because the tests never get near, and a production system would use a `u64`.

## In real code

### Using it: three clocks you will write or meet

```rust test
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A logical clock shared by threads: unique, increasing, no relation to real time.
struct LogicalClock(AtomicU64);
impl LogicalClock {
    fn tick(&self) -> u64 { self.0.fetch_add(1, Ordering::Relaxed) }     // Relaxed is enough: only uniqueness matters, not ordering with other memory
}

#[test]
fn every_thread_gets_unique_increasing_stamps() {
    let clock = Arc::new(LogicalClock(AtomicU64::new(0)));
    let handles: Vec<_> = (0..8).map(|_| {
        let c = Arc::clone(&clock);
        std::thread::spawn(move || (0..1000).map(|_| c.tick()).collect::<Vec<_>>())
    }).collect();
    let per_thread: Vec<Vec<u64>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for stamps in &per_thread { assert!(stamps.windows(2).all(|w| w[0] < w[1]), "each thread sees its own stamps increase"); }
    let mut all: Vec<u64> = per_thread.into_iter().flatten().collect();
    all.sort();
    assert_eq!(all, (0..8000).collect::<Vec<_>>(), "no ties and no gaps across threads");
}

#[test]
fn a_single_threaded_counter_is_deterministic() {
    // The replacer's clock: replaying the same calls gives the same stamps, so a test can assert which frame is evicted.
    fn run() -> Vec<usize> { let mut now = 0; (0..5).map(|_| { let t = now; now += 1; t }).collect() }
    assert_eq!(run(), run());
}
```

```rust test
/// Lamport clocks: each process counts its own events and, on receiving a message,
/// jumps past the sender's stamp. If A happened before B then stamp(A) < stamp(B).
#[derive(Default)]
struct Lamport { t: u64 }
impl Lamport {
    fn event(&mut self) -> u64 { self.t += 1; self.t }
    fn send(&mut self) -> u64 { self.event() }                           // the stamp travels with the message
    fn receive(&mut self, msg: u64) -> u64 { self.t = self.t.max(msg) + 1; self.t }
}

#[test]
fn receiving_a_message_orders_the_events_across_processes() {
    let (mut a, mut b) = (Lamport::default(), Lamport::default());
    for _ in 0..5 { b.event(); }                                         // B is "ahead" locally
    let sent = a.send();                                                 // A's first event: stamp 1
    let received = b.receive(sent);
    assert!(received > sent, "the receive happens after the send");
    assert_eq!(received, 6);                                             // max(5, 1) + 1
    let local_a = a.event();
    assert!(local_a < received, "A's later local event is NOT ordered after B's receive: stamps only give happens-before one way");
}

#[test]
fn what_the_real_clocks_promise() {
    use std::time::{Instant, SystemTime, UNIX_EPOCH};
    let t0 = Instant::now();
    let t1 = Instant::now();
    assert!(t1 >= t0, "Instant is monotonic: it never goes backwards (but ties are possible)");
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    assert!(secs > 1_700_000_000, "SystemTime is wall-clock time since 1970; it can be stepped by NTP, so never use it for ordering");
    // A coarse clock ties; a (coarse, counter) pair does not.
    let coarse = |t_ns: u64| t_ns / 1_000_000;                           // 1 ms resolution
    assert_eq!(coarse(1_000_100), coarse(1_900_000), "two events 0.8 ms apart read the same time");
}
```

### In the exercises

- **1d-01, 1d-02:** the replacer's `current_timestamp`: read, then increment, exactly as in `tick` above. Distinct timestamps are what make every comparison in the LRU-K rule unambiguous.

### Where it is used

- **Id and sequence generators**: `AtomicU64::fetch_add` behind task ids, span ids (`tracing`), connection ids and `Arc`-shared counters in nearly every async runtime.
- **Databases**: PostgreSQL transaction ids (`xid`) and MVCC snapshots, log sequence numbers (LSNs) in every write-ahead log, and the timestamps module 4's MVCC uses are all logical clocks.
- **Distributed systems**: Lamport timestamps and vector clocks (Dynamo-style stores) track causality; Google Spanner uses *real* time with a stated uncertainty (TrueTime) because logical clocks cannot order events between machines that never communicate.
- **Hybrid logical clocks** (CockroachDB, YugabyteDB) pair a wall-clock reading with a counter so timestamps are both close to real time and tie-free: the `(coarse, counter)` idea in the last test.
