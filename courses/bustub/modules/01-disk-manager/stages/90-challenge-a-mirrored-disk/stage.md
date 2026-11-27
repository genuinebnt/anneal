A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`MirroredDisk` in `src/storage/disk/mirrored_disk.rs`: a disk made of two other disks that keeps the same pages on both, so that one of them failing does not lose anything. It is a `DiskIo` like the others.

## Why

Real storage engines do not trust one device. Mirroring (RAID 1) is the simplest answer, and a test of whether you understand what a disk promises: a disk that fails for a while and comes back has *missed* writes, and a naive mirror will read the old page from it.

## The contract

- Writes go to both disks; a write succeeds if at least one disk took it, and fails only if both failed.
- Reads return the latest successful write of the page (zeros if never written or deleted), even while one disk is down and even if a disk missed writes while it was down.
- At most one disk is down at any moment. When a disk comes back, the reads that follow bring it up to date.
- Deleting a page deletes it from both.

## Invariants

These must hold after every step, whatever the input:

- With both disks healthy, both copies hold every write.
- No read ever returns a page older than the latest successful write.
- A write that returns `Ok` is on at least one disk.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Taking either disk down and reading gives the same bytes as before.
- Writing while a disk is down, bringing it back, and then taking the *other* disk down still reads the new page.
- The mirror behaves exactly like a plain map of pages.

## Examples

Worked cases (the tests include them):

```text
write 1; primary down; write 2; primary up; read -> 2 (not 1)
both down; write -> error
delete after write -> zeros
```

## What the tests check

- A mirror with both disks healthy behaves like one disk.
- Reads survive either disk failing; writes survive one disk failing.
- A disk that missed a write never serves the old page.
- A property over random writes, reads, deletes and failures against a map of pages.

## Done when

All the `s1a_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Disk-head scheduling](/cookbook/p/disk-scheduling-raid): RAID 1 mirroring with degraded reads, rebuild and a checksummed mirror.
