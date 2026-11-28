> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 and 2**. Solve those first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 3 of 9.** Now the filesystem exists on the disk. This step writes its empty form, mounts it, hands out **inodes** and **data blocks**, and groups changes into **transactions**.

An inode is the record of one file or directory. It has no name.

**Given in this step's starter**, ready to use: the `Fs` struct, `format` (with its helper `assemble`), `bit` and `set_bit`. Also the `Inode` record's `encode` and `decode`, and two private helpers. `data_block(n)` refuses a number outside the data area, and `inode_place(i)` finds a record or refuses the number.

You write everything marked `todo!()`, and the private helpers you need, such as `commit`. `format` calls your code only through the `Fs` it returns. Its rules are below because the tests and later steps rely on them.

## Format and mount

`Fs::format(dev, g)` writes the metadata of an empty filesystem and returns it mounted. It returns `BadGeometry` if the layout is refused or the device has fewer blocks than `g.total_blocks`.

- Every block before `data_start` is overwritten. Data blocks are left as they are.
- The journal's first block holds `JMAGIC` (u32) and zeros. The rest of the journal is zeros.
- In the **data bitmap**, bit `b` stands for block `b` of the disk. The bits below `data_start` are set.
- In the **inode bitmap**, bit `i` stands for inode `i`. Only bit 0 is set: there is no inode 0.
- Bit `i` of a bitmap is bit `i % 8` of byte `i / 8`, counting from its first block.
- The inode table is zeros.

**Order matters**, because format may be stopped halfway over an old filesystem. First overwrite block 0 with zeros and `sync`. Block 0 is written once more, with the superblock, as the **last write**: a `sync` comes just before it and just after it.

`mount(dev)` reads and checks the superblock. A device with no blocks is `BadMagic`. A device smaller than the superblock says is `BadGeometry`.

`mount_with_clock` is the same with your own clock. `mount` and `format` use `TickClock(0)`.

## Transactions

`tx(change)` runs the closure. All block writes inside it wait in the cache.

- If it returns `Ok`, they are **committed**: here, flushed to the device and then synced. (Step 8 replaces this with something that survives a power cut.)
- If it returns `Err`, they are forgotten and the device is never written. The error is returned.
- The clock is asked once per transaction. That time is the `now` of every change in it.
- A `tx` inside a `tx` returns `Err(Invalid(..))` and leaves the outer one open.
- `write_block`, `alloc_block` and `alloc_inode` outside a transaction return `Err(Invalid(..))`. The other changing calls end in `write_block`, so they refuse too.
- **If the commit fails**, nobody knows what is on the disk. Return the error. From then on, every `tx` and every call that reads a block returns `Err(Poisoned)`. Only a new mount helps.

## Blocks

- `read_block` and `write_block` go through the cache (capacity `CACHE_BLOCKS`). A number of `total_blocks` or more is `Corrupt`.
- `bit(start, i)` and `set_bit(start, i, value)` read and change bit `i` of the bitmap that starts at block `start`.
- `alloc_block` takes the **first free block at or after the cursor**, going round to the start of the disk once. The cursor is `data_start` after a mount, and one past the block last taken. It marks the block, writes zeros over it and returns it. With none free it returns `NoSpace`.
- A damaged bitmap may show a block before `data_start` as free. If the search finds one, return `Corrupt`.
- `free_block(n)` clears the bit. A block outside the data area, or one that is already free, is `Corrupt`.
- `free_blocks()` counts: `total_blocks` less the set bits among the first `total_blocks` bits.
- The tests do not rely on where either cursor is after a transaction that failed.

## Inodes

Record `i` is in the inode table at byte `i * 128`. All numbers are little-endian; unlisted bytes are zero.

| Offset | Type | Field |
|---|---|---|
| 0 | u8 | `kind`: `K_FREE`, `K_FILE` or `K_DIR` |
| 2 | u16 | `nlink` |
| 4 | u16 | `mode` |
| 8, 16, 24 | u64 each | `size`, `mtime`, `ctime` |
| 32 | 12 x u32 | `direct` |
| 80, 84 | u32 each | `indirect`, `double` |

- `load_inode(i)` returns `Corrupt` if `i` is 0 or above `inode_count`, if the kind is none of the three, or if `size` is above `MAX_FILE_SIZE`. It does not look at the bitmap.
- `store_inode` refuses the same numbers.
- `alloc_inode(kind, nlink)` takes the first free number at or after its cursor, going round once. The cursor is 1 after a mount, and one past the number last taken. With none, `NoInodes`. The new record has that kind and link count, mode `0o755` for a directory and `0o644` otherwise, `mtime` = `ctime` = now, and zeros elsewhere.
- `free_inode(i)` writes a blank record and clears the bit.
- `adjust_links(i, change)` adds to `nlink` and sets `ctime`. A result below 0 or above `u16::MAX` is `Invalid(..)`.

Any message inside `Corrupt` or `Invalid` is fine. No call may panic on these inputs.
