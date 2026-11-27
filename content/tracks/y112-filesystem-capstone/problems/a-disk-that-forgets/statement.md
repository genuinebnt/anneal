> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 6**. Solve those first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 7 of 9.** The next step makes the filesystem survive a power cut. You cannot pull a plug in a test, so first you build a disk that misbehaves like a real one, on demand and repeatably.

A real disk keeps writes in its own memory. Until a `sync`, a power cut may lose any of them, keep them in another order, or leave a block **half written**.

## `FaultDisk`

It is a memory disk plus the writes that no `sync` has covered yet (the **pending** writes).

- `new(blocks)` is zeros; `from_image(bytes)` starts from these bytes, all of them safe.
- `write_block` records a pending write. `read_block` returns the newest content of the block.
- `sync` makes all pending writes safe, in the order they were made.
- A block number past the end is `Err(FsError::Io(..))`, and does not count as a write.

**Arming.** `arm(Some(k))` sets the write count to 0, and the next `k` writes succeed. Once they have, the **next write or sync** fails with `Crashed`. That write is not recorded, and that sync makes nothing safe. From then on every read, write and sync fails with `Crashed`.

Failing the sync matters. It is the only way to cut the power while a whole group of writes is still in doubt.

 `arm(None)` only sets the count to 0. `writes()` counts successful writes since the disk was made or last armed. Syncs do not count.

## After the cut

`post_crash_image(mode)` returns the bytes found on the disk after the power fails now. It changes nothing, and works whether or not the disk has crashed.

- `Prefix`: every pending write landed, in order. (Together with `arm(Some(k))` this gives "the power failed after exactly k writes".)
- `Chaos(seed)`: decided by the seed. The same seed and the same history give the same bytes.

In `Chaos` mode, a disk is made of **sectors** of 512 bytes, which are never torn inside. Each pending write is, independently: **lost**; **whole**; or **torn**, so that each of its 8 sectors independently lands or not. The surviving parts are applied to the safe content **in a random order**, not necessarily the order they were written.

Any random generator of your own will do (do not use a crate). The tests do not expect particular images. Over 2000 seeds they expect that:

- every sector holds its safe content or that of a pending write to that block;
- one pending write is seen lost, whole and torn, and each sector is seen both ways in a torn block;
- writes to two blocks are lost in all four combinations;
- of two pending writes to one block, either may be the one left, and neither is more than twice as common as the other.
