> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 7**. Solve those steps first.
>
> It also gives a `FileDevice`, which keeps an image in a real file, as in the preparation's file of numbered pages.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 8 of 9.** A `mkdir` changes several blocks. Today `commit` writes them one by one to their places. Cut the power in the middle and the disk holds half a directory.

Make every transaction **all or nothing**, with the journal: blocks `journal_start` to `journal_start + journal_blocks - 1`. You change two functions in the reference code, `commit` and `open_device` (which `mount` calls).

## The journal's blocks

Call the journal's first block `J`. A transaction of `n` changed blocks uses:

| Block | Content |
|---|---|
| `J` | **header**: `JMAGIC` u32 at 0, sequence number u64 at 8, `n` u32 at 16, then at 32 + 4i the home block (u32) of copy `i` |
| `J + 1 ..= J + n` | the **copies**: the new content of each changed block |
| `J + 1 + n` | **commit record**: `CMAGIC` u32 at 0, the same sequence number u64 at 8, a checksum u32 at 16 |

The checksum is the CRC-32 of the whole header block followed by all the copies, in order. Unlisted bytes are zero. Copies go by rising home block number.

A **clean header** has `JMAGIC`, the sequence number of the last transaction, and `n` = 0.

## Commit

A transaction that changed nothing makes no device call. If `n > journal_blocks - 2`, return `TxTooBig`: nothing is written, the changes are dropped, and the filesystem **keeps working**.

Otherwise, the device calls are exactly these, in this order:

1. write the header, then the copies;
2. `sync`;
3. write the commit record;
4. `sync` (from here on the transaction has happened);
5. write each changed block to its home, by rising block number;
6. `sync`;
7. write the clean header;
8. `sync`.

The new sequence number is the last one **plus 1, wrapping** to 0 after `u64::MAX`. If any of these calls fails, return its error and poison the filesystem, as in step 3.

## Recovery, at every mount

Read the superblock, then the header.

- Without `JMAGIC`, or with `n` = 0: **write nothing**. The sequence number is 0 without the magic, else the header's.
- Otherwise, the transaction is **complete** only if all of these hold:
  - `n <= journal_blocks - 2`;
  - the commit record has `CMAGIC`, the header's sequence number and a matching checksum;
  - **every** home block is at least `ibm_start` and below `total_blocks`.
- If complete, write every copy to its home, then `sync`.
- In both cases, then write a clean header with the header's sequence number and `sync`.

The journal's content is untrusted: `n` may be `u32::MAX`.

## What the tests do

They build journals by hand and compare your calls with the list above. They also cut the power with the step 7 disk (given here) after **every write** of every kind of call, in both modes, and during recovery too. After each cut the mounted tree and the free block count must be exactly the old ones or the new ones. Once a call has returned `Ok`, they must be the new ones.

**Two syncs no crash can prove.** With a checksum over the copies, sync 2 is not needed for safety: a commit record that arrives before its copies fails the check. Losing the clean header of sync 8 only makes recovery do the same work again. The same holds for the last sync of recovery.

No power cut can show that these are missing, so only the call list grades syncs 2 and 8. Nothing grades recovery's last sync. The cuts do show that syncs 4 and 6, and recovery's sync after the copies, are needed.

**What this does not give you.** A long `write` is several transactions, so a cut can leave a prefix of it. A disk that lies about `sync`, or damages blocks later, is outside this model.
