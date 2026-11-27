# bustub-rs

[BusTub](https://github.com/cmu-db/bustub) (CMU 15-445/645's teaching database) rebuilt in Rust, one small stage at a time, in the
style of CodeCrafters. The layout, file names and tests mirror BusTub's; the code is yours.

```
anneal course status      # where you are
anneal course show        # the current stage: task, tests, syntax, links to read and watch
anneal course test        # run its tests (and the earlier stages' as a regression check)
anneal course next        # the next stage
git commit -am "..." && git push    # the pre-push hook runs `anneal course test` and records the result
```

**Your way.** The tests call only the public API each stage names. How you build the inside is up to you: the structs and helpers in the template are a starting point, not a contract. Change them, replace them, write it your own way; what has to hold is that the tests pass.

Work in `src/`. Every function you have to write is `todo!()` until its stage. `tests/` holds the stage tests (`stages_*.rs`) and
BusTub's own tests, ported (`*_test.rs`); they are yours to read. macOS and Linux only (the disk manager uses `pread`/`pwrite`).

Unit of work: a **stage** is one self-contained piece of a component: tens of lines and at least one design decision, with its own
tests. Stages of a component are grouped into a **module** (the disk manager, the buffer pool, ...); the last stage of a module is
BusTub's own test for it. Stages numbered 90 and up are optional challenges: no solution, their own tally, skip them freely.

Tests and parts of the structure are adapted from BusTub, Copyright (c) 2015-2025 Carnegie Mellon University Database Group, MIT licence.
