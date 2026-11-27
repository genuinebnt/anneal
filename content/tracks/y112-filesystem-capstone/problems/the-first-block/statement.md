**Capstone, step 1 of 9.** You are building a small UNIX filesystem that lives on a disk of 4096-byte blocks. Nothing above the disk knows whether it is memory or a real file.

This step has the disk, a checksum and the first block. That first block, the **superblock**, says where everything else is. Mounting trusts it, so it must refuse anything that is not exactly right.

## The disk

`BlockDevice` is given. Write `MemDisk`, a disk in memory.

- `new(blocks)` is that many blocks of zeros. `from_image(bytes)` holds those bytes. Only whole blocks count in `num_blocks`.
- `image()` returns all the bytes, as given or as since changed.
- Reading or writing a block that is not wholly on the disk returns `Err(FsError::Io(..))` with any message. This holds for every `u64`. It never panics.
- `sync` succeeds and does nothing.

## The checksum

`crc32` is the common CRC-32 (zlib, PNG): reflected polynomial `0xEDB88320`, start value all ones, result inverted. `crc32(b"123456789")` is `0xCBF43926`.

`crc32_extend(crc32(a), b)` equals the CRC of `a` followed by `b`. The tests cut data at random places.

## The layout

Blocks are used in this order. Each part starts where the one before ends.

| Part | First block | Size in blocks |
|---|---|---|
| superblock | 0 | 1 |
| journal | `journal_start` = 1 | `journal_blocks` |
| inode bitmap | `ibm_start` | 1 |
| data bitmap | `dbm_start` | one bit per block of the whole disk, rounded up, at least 1 |
| inode table | `itable_start` | `inode_count + 1` records of 128 bytes, rounded up |
| data | `data_start` | the rest |

A bitmap block holds `BITS` = 32768 bits. Inode numbers start at 1, so the table has one record that is never used.

`Super::layout` returns `Err(FsError::BadGeometry)` when:

- `inode_count` is 0, or 32768 or more (the one bitmap block must hold a bit for each, and one for number 0);
- `journal_blocks` is below 12 or above 1000;
- `total_blocks` is above `u32::MAX` (block numbers are stored in 32 bits);
- fewer than 8 blocks are left for data.

Any values may be passed, up to the largest of each type.

## The superblock

All numbers are little-endian. Bytes not listed are zero.

| Offset | Type | Value |
|---|---|---|
| 0 | u64 | `MAGIC` |
| 8 | u32 | `VERSION` (1) |
| 12 | u32 | the block size, 4096 |
| 16 | u64 | `total_blocks` |
| 24 | u32 | `inode_count` |
| 28 | u32 | `journal_blocks` |
| 32, 40, 48, 56, 64 | u64 each | `journal_start`, `ibm_start`, `dbm_start`, `itable_start`, `data_start` |
| 72 | u32 | CRC-32 of bytes 0 to 71 |

`decode` checks in this order and returns the first failure:

1. wrong magic: `BadMagic`;
2. wrong checksum: `BadChecksum`;
3. anything else wrong: `BadGeometry`. That means another version or block size, a geometry that `layout` refuses, or stored positions that differ from the ones `layout` computes.

It reads only bytes 0 to 75. A correct checksum proves nothing about intent: the tests change fields and seal the block again.
