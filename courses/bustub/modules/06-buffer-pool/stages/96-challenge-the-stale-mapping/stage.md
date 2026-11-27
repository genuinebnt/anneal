A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/buffer/page_table.rs` keeps which frame holds which page in two maps (page to frame, frame to page) that must always agree. It looks right, and after some sequences it answers that a frame holds a page it no longer holds. Find the bug and fix it.

## Why

A buffer pool lives on this mapping, and two maps that mirror each other are a classic source of bugs: every operation has to update both, and the one that forgets leaves a stale entry that is only noticed when a frame is reused. The invariant to test is the mirror itself.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Removing a page dropped it from the page-to-frame map and left the frame-to-page entry behind, so the frame still claimed the old page. Both halves of the mirror change together.||

## The contract

- `insert(page, frame)` records that `frame` holds `page`; if the page was in another frame, or the frame held another page, the old pairing is dropped.
- `remove_page(page)` forgets the page and its frame; returns the frame.
- `frame_of(page)` and `page_of(frame)` answer from the maps; `len()` is the number of pairs.

## Invariants

These must hold after every step, whatever the input:

- `frame_of(p) == Some(f)` exactly when `page_of(f) == Some(p)`.
- Each page is in at most one frame and each frame holds at most one page.
- `len()` is the number of pages known, and the number of frames known.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `remove_page(p)` makes both `frame_of(p)` and `page_of(its old frame)` empty.
- Inserting then removing a page leaves the table as it was.
- The table equals a set of (page, frame) pairs with both columns unique.

## Examples

Worked cases (the tests include them):

```text
insert(1, A); remove_page(1); insert(2, A) -> page_of(A) = 2, frame_of(1) = None
insert(1, A); insert(1, B) -> A is free
```

## What the tests check

- Insert, remove, replace.
- A frame reused after its page was removed.
- A property: the mirror invariant after every step.

## Done when

All the `s1f_c5` tests pass, and you can say in one sentence what the bug was.
