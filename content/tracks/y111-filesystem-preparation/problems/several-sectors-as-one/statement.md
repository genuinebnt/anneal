A program keeps records in the **sectors** of a small disk. One action often changes several records, such as moving a seat from one list to another. If the power fails halfway, one list has changed and the other has not.

Write `update` and `recover`.

- `update(disk, changes)` writes each `(sector number, new content)`.
- After a power cut, the machine restarts and calls `recover(disk)` before anything else.

## The promise

After a power cut **at any moment**, and a `recover`, the data sectors are **all as before the update, or all as after it**. Never a mix, and never anything else.

- If `update` returned `Ok`, every later cut and `recover` shows the new content.
- The power can fail again during `recover`. Running it again must still keep the promise.
- This holds for every update in a long history, whatever earlier ones left behind.

## The disk

Sectors are 64 bytes. Sectors `0..SPARE` (24 of them) are **yours** to use as you like, and the tests never look at what you keep there. The data is from `SPARE` on. All spare sectors are zero on a new disk, but `recover` must also cope with any bytes there: it then leaves the data alone.

A write may stay in the disk's own memory until `sync`. When the power fails, **each write since the last sync** may be lost, whole, or **torn**: any of its four 16-byte quarters may land without the others. The writes may land in **any order**. A torn sector is not reported as an error.

Everything you need to tell a whole record from a torn one, you must make yourself, with the standard library. A simple hash is enough here.

## Errors

Checked before anything is written, in this order:

1. more than `MAX_CHANGES` (8) changes: `TooMany`;
2. a sector number below `SPARE` or past the disk, or one listed twice: `BadSector`.

When a disk call fails, return `PowerLost`. An empty list is allowed. `update` writes only spare sectors and the listed ones. `recover` writes only spare sectors and those of the interrupted update.

## How you are tested

The tests make the power fail before every write and every sync. For each stop they try every choice of which unsynced writes landed whole, every way of tearing one of them, and many random mixes. They do this to your `recover` as well.
