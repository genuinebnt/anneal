> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 3**. Solve those first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 4 of 9.** An inode has room for 14 block numbers. A file can still be about 4 GiB long, and a file with one byte at the far end should cost almost nothing.

## Where a file's blocks are

File block `i` holds bytes `i * 4096` and on.

| File blocks | Found through |
|---|---|
| 0 to 11 | `direct[i]` |
| the next `PER_BLOCK` = 1024 | slot `i - 12` of the block `indirect` |
| the next 1024 x 1024 | with `j = i - 12 - 1024`: slot `j % 1024` of the block named by slot `j / 1024` of the block `double` |

A **pointer block** is 1024 numbers, each a little-endian u32. The number **0 means a hole**: nothing is allocated there, and it reads as zeros.

All pointers on the disk are untrusted. Whenever you follow or free one, a block outside the data area is `Corrupt`.

## What to write

**Given in this step's starter:** `file_blocks` (with its helper `collect`), `read_at` and `write_at`. They all stand on `block_of`. You write `block_of` and `resize`, with the private helpers they need. The rules for the given functions are listed too, because the tests check them together with yours.

- `block_of(inode, i, alloc)` returns the disk block of file block `i`, or 0 for a hole. With `alloc`, it allocates what is missing: pointer blocks on the way and the data block.
- `block_of` changes `inode` in memory only. `i >= MAX_FILE_BLOCKS` is `FileTooBig`. Without `alloc` it changes nothing.
- `file_blocks(inode)` lists every block the inode owns, data and pointer blocks, in any order. If there would be **more than the disk has blocks**, return `Corrupt` early. Damaged pointer blocks can name millions.
- `read_at(inode, offset, len)` returns at most `len` bytes, ending at the file's size. At or past the end it returns nothing. Reading allocates nothing.
- `write_at(ino, inode, offset, data)` writes, grows `size` if needed, sets `mtime` and `ctime` to now, and stores the inode. A write that would end past `MAX_FILE_SIZE` (or overflow) is `FileTooBig` and changes nothing. **Empty `data` changes nothing at all**, even the size.
- `resize(ino, inode, size)` sets the size, sets both times to now and stores the inode. Above `MAX_FILE_SIZE` is `FileTooBig`.

`write_at` and `resize` run inside the caller's transaction. If they fail, for example with `NoSpace`, the transaction throws everything away.

## What shrinking frees

When the new size is smaller:

- every **data** block that lies wholly past the new end is freed;
- a **pointer** block is freed exactly when **every file block it could cover** is cut, whether or not it still names anything. One that covers any kept block stays;
- each freed block's number is set to 0 wherever it was stored;
- the cut bytes of the last kept block are zeroed, so that growing the file again reads zeros.

A file block is "kept" if it is below the new size divided by 4096, rounded up.

Speed is not graded. The tests use files with data near 4 GiB, mostly holes.
