---
title: The CLOCK algorithm: a second chance with one bit
summary: How a ring, a hand and a reference bit approximate LRU without touching a list on every hit, and the edge cases (removal, the hand position) that the stage tests probe.
minutes: 7
---
LRU is expensive because it must record *every* use. CLOCK records the *fact* of use in a single bit and looks at it only when it has to evict.

## The structure

The evictable frames sit on a **ring**. Each frame has a **reference bit**; a **hand** points at one of them. The replacer is told two things:

- `unpin(frame)`: the frame is now evictable (nobody is using it). Add it to the ring with its bit **set** (it was just used), or set the bit if it is already there.
- `pin(frame)`: somebody needs it: take it off the ring, whatever its bit.

And asked one thing: `victim()`.

## The sweep

```text
loop:
    if the frame at the hand has its bit set:    clear the bit, move the hand on     (a second chance)
    else:                                         remove this frame and return it     (the victim)
```

A frame gets a **second chance**: the first time the hand passes it, it survives with its bit cleared; if it is *used again* before the hand returns, the bit is set again and it survives again; if not, the next pass takes it. In the worst case every bit is set and the hand goes round the whole ring once, clearing them, then evicts the first one: bounded, at most `n + 1` steps.

```svg
caption: The hand starts at A. A and B have their bit set, so the hand clears each and moves on (a second chance). C's bit is clear, so C is the victim. Frames D, E, F are not touched. (Animated.)
<svg viewBox="0 0 760 250" role="img" aria-label="A ring of six frames with reference bits and a hand sweeping to the first frame with a clear bit">
<style>
@keyframes ck-h{0%,18%{transform:rotate(0deg)}24%,46%{transform:rotate(60deg)}52%,100%{transform:rotate(120deg)}}
@keyframes ck-a{0%,22%{fill:var(--grn)}28%,100%{fill:var(--line)}}
@keyframes ck-b{0%,50%{fill:var(--grn)}56%,100%{fill:var(--line)}}
@keyframes ck-v{0%,70%{opacity:0}78%,100%{opacity:1}}
.ck-hand{transform-origin:380px 120px;animation:ck-h 9s ease-in-out infinite}
.ck-a{animation:ck-a 9s linear infinite}.ck-b{animation:ck-b 9s linear infinite}.ck-v{animation:ck-v 9s linear infinite}
</style>
<circle class="never" cx="380" cy="120" r="70"/>
<circle class="box" cx="380.0" cy="50.0" r="22"/><text class="mid fg" x="380.0" y="48.0">A</text><circle class="ck-a" cx="380.0" cy="62.0" r="5" style="fill:var(--grn)"/><circle class="box" cx="440.6" cy="85.0" r="22"/><text class="mid fg" x="440.6" y="83.0">B</text><circle class="ck-b" cx="440.6" cy="97.0" r="5" style="fill:var(--grn)"/><circle class="hot" cx="440.6" cy="155.0" r="22"/><text class="mid fg" x="440.6" y="153.0">C</text><circle cx="440.6" cy="167.0" r="5" style="fill:var(--line)"/><circle class="box" cx="380.0" cy="190.0" r="22"/><text class="mid fg" x="380.0" y="188.0">D</text><circle cx="380.0" cy="202.0" r="5" style="fill:var(--grn)"/><circle class="box" cx="319.4" cy="155.0" r="22"/><text class="mid fg" x="319.4" y="153.0">E</text><circle cx="319.4" cy="167.0" r="5" style="fill:var(--line)"/><circle class="box" cx="319.4" cy="85.0" r="22"/><text class="mid fg" x="319.4" y="83.0">F</text><circle cx="319.4" cy="97.0" r="5" style="fill:var(--grn)"/>
<g class="ck-hand"><line x1="380" y1="120" x2="380" y2="76" class="ln-w"/><circle cx="380" cy="120" r="5" style="fill:var(--warn)"/></g>
<text class="dim sm" x="20" y="30">bit set</text><circle cx="64" cy="26" r="5" style="fill:var(--grn)"/>
<text class="dim sm" x="20" y="50">bit clear</text><circle cx="72" cy="46" r="5" style="fill:var(--line)"/>
<text class="t-w sm" x="500" y="60">hand: the next frame to look at</text>
<g class="ck-v"><text class="t-g big" x="500" y="150">victim: C</text><text class="dim sm" x="500" y="170">A and B survived with their bit cleared</text></g>
</svg>
```

## Edge cases the tests probe

- **The hand after a removal.** When a frame is removed from the ring, the hand must not skip the next frame or point past the end: if the removed frame was *before* the hand in the ring, the hand's index decreases by one; if the hand now points past the last frame it wraps to 0.
- **`unpin` of a frame already on the ring** only sets the bit. It must not add a second copy.
- **An empty ring** has no victim: `victim()` returns `None` rather than looping.
- **The ring's capacity** is the number of frames the replacer was made for; adding more is a bug (BusTub asserts).

## CLOCK versus LRU

| | LRU | CLOCK |
|---|---|---|
| on a hit | move the node to the back (a list write, usually under a lock) | set one bit |
| to evict | `pop_front` | sweep from the hand |
| accuracy | exact recency | recency within one revolution |
| scan resistance | none | none (a scan sets every bit once) |
| used in | many textbook buffer pools | operating-system page replacement, many embedded caches |

BusTub's classic replacer assignment asks for both so that you meet the trade-off yourself: LRU is simplest to reason about, CLOCK is the one that scales.

## Invariants: what must always be true

- **Only evictable frames are on the ring.** `unpin` adds a frame with its bit set; `pin` takes it off whatever its bit; `unpin` of a frame already on the ring only sets its bit and never adds a second copy.
- **The hand points at a frame on the ring** (or the ring is empty). After a removal it neither skips the next frame nor points past the end: it moves back by one if the removed frame was before it, and wraps to 0 at the end.
- **A sweep ends.** Each turn of the hand clears every bit it passes, so a non-empty ring yields a victim within two turns.
- **An empty ring has no victim:** `victim()` returns `None` instead of looping.
- **The ring never holds more frames than the replacer was made for.**

## In real code

### Using it: a complete, runnable CLOCK replacer

```rust test
struct Clock {
    ring: Vec<(u32, bool)>,                                           // (frame, reference bit), in ring order
    hand: usize,
    capacity: usize,
}

impl Clock {
    fn new(capacity: usize) -> Self { Clock { ring: vec![], hand: 0, capacity } }

    fn unpin(&mut self, frame: u32) {                                 // "evictable now, and it was just used"
        match self.ring.iter_mut().find(|(f, _)| *f == frame) {
            Some(slot) => slot.1 = true,                              // already on the ring: only set the bit
            None => { assert!(self.ring.len() < self.capacity); self.ring.push((frame, true)); }
        }
    }

    fn pin(&mut self, frame: u32) {                                   // "in use: not evictable"
        let Some(at) = self.ring.iter().position(|(f, _)| *f == frame) else { return };
        self.ring.remove(at);
        if at < self.hand { self.hand -= 1; }                         // the three hand cases
        if self.hand >= self.ring.len() { self.hand = 0; }
    }

    fn victim(&mut self) -> Option<u32> {
        if self.ring.is_empty() { return None; }
        loop {                                                        // terminates: each pass clears a bit
            if self.ring[self.hand].1 {
                self.ring[self.hand].1 = false;                       // second chance
                self.hand = (self.hand + 1) % self.ring.len();
            } else {
                let (f, _) = self.ring.remove(self.hand);
                if self.hand >= self.ring.len() { self.hand = 0; }
                return Some(f);
            }
        }
    }
}

#[test]
fn the_worked_example_from_the_figure() {
    let mut c = Clock::new(6);
    for f in 0..6 { c.unpin(f); }                                     // A..F enter with their bits set
    c.ring[2].1 = false;                                              // frame 2 ("C") was not used since the hand last passed
    assert_eq!(c.victim(), Some(2));                                  // A and B get a second chance; C is the victim
    assert_eq!(c.ring.iter().map(|(f, b)| (*f, *b)).collect::<Vec<_>>(),
               vec![(0, false), (1, false), (3, true), (4, true), (5, true)]);
}

#[test]
fn pin_moves_the_hand_correctly() {
    let mut c = Clock::new(4);
    for f in 0..4 { c.unpin(f); }
    c.hand = 2;
    c.pin(0);                                                         // removed before the hand: the hand index moves down so it still points at frame 2
    assert_eq!(c.ring[c.hand].0, 2);
    c.pin(3);                                                         // the last element
    assert!(c.hand < c.ring.len());
    assert_eq!(c.victim().is_some(), true);
}
```

### In the exercises

- **1c-03:** the ring, the hand, the reference bit and the three hand-after-removal cases are the stage; the worked sweep above is the check to make against your own picture.



### Where it is used

- **Operating-system kernels**: BSD, Linux (as the "second chance" for the inactive list) and Windows' working-set trimming use CLOCK variants because a hit is a bit-set with no lock.
- **PostgreSQL's buffer manager**: a *clock sweep* over `usage_count`s (a multi-bit generalisation: decrement on each pass, evict at zero).
- **WiredTiger and other embedded stores** use CLOCK-style eviction for cache pages.
- **Hardware**: many CPU caches' "not recently used" bits.
