> **Spoiler before you open the editor.** The starter holds reference code for **capstone step 1** (the disk and the superblock). Solve that step first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 2 of 9.** The filesystem will often change several blocks for one operation. If one of them cannot be had, none may reach the disk. So changes are first held in memory.

`Cache<D>` sits in front of a device. All later reads and writes go through it.

- `read(n)` returns the block as the filesystem sees it now. If the block is held, the device is **not** called. If not, it is read once and then held. A failed read returns the error and holds nothing.
- `write(n, block)` replaces what is held for `n`. It calls the device **not at all**, and marks the block **changed**.
- `dirty_blocks()` lists the changed blocks with their new content, by rising block number.
- `flush()` writes the changed blocks to the device, by rising block number, and marks each unchanged once written. It does not call `sync`.
- If a write fails during `flush`, it returns that error at once. That block and all after it **stay changed**.
- `discard()` forgets every changed block. The next read of one asks the device again.
- `len()` is how many blocks are held. `device()` lends the device; `into_device()` gives it back and forgets the rest.

## How much it holds

`capacity` is the number of blocks the cache tries to keep. A changed block is **never** forgotten before it is flushed or discarded, even if the cache grows far past its capacity.

After every `read`, `write` and `flush`: while more than `capacity` blocks are held and at least one is unchanged, forget the unchanged block that was **used longest ago**. A use is a `read` or a `write` of that block. `flush` is not a use.

So when everything else is changed, even the block just read may be forgotten at once. A capacity of 0 is allowed.

Speed is not graded: finding the oldest block by looking at all of them is enough here.
