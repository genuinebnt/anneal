> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 8**. Solve those first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 9 of 9.** The journal protects against power cuts. It does not protect against a bad cable, a bug of yours or a flipped bit. Write `fsck`, which checks the whole filesystem, says what is wrong and repairs **only what it can repair without risk**.

**Given in this step's starter:** `compare`, which takes what your walk found and notes every difference from the link counts and the two bitmaps. Also `apply_fix`, which makes the one change that fixes one problem, inside your transaction. You write the walk (filling `Walk`) and `fsck`: calling these, sorting, deciding what may be repaired and committing it.

## The walk

Start at the root and follow every entry. Entries named `.` and `..` are counted but never followed. Each directory is walked once, however often it is named. The disk may be damaged in any way: `fsck` must always end, and never panic.

An inode is **reached** if the walk could load it. A reached inode claims all the blocks that `file_blocks` lists.

An entry counts towards an inode's **names** only if it names a record that loads and is not `K_FREE`.

**The inode's record says whether it is in use, not the bitmap.** An entry that names a live record is followed even if the inode's bit is clear.

## Problems

`Report::problems` is sorted and has each problem once.

| Problem | When |
|---|---|
| `BadDirectory{ino}` | a directory to be walked (the root included) cannot be loaded, or its block tree or entries cannot be read as in step 5 (a free or file record counts) |
| `BadEntry{dir, name}` | an entry names a number out of range, a free record or a damaged record |
| `BadBlockTree{ino}` | a reached file's block tree cannot be read |
| `DuplicateName{dir, name}` | a directory uses a name twice |
| `WrongDots{dir, name}` | `.` does not name the directory, or `..` does not name its parent (the root is its own parent) |
| `MissingDots{dir}` | a readable directory lacks `.` or `..` |
| `DirectoryTwice{ino}` | a second entry other than the dots names a directory that is already named (or the root) |
| `CrossLink{block}` | a block is claimed more than once, even by one file |
| `LinkCount{ino, stored, found}` | a reached inode that loads has another link count than its number of names |
| `OrphanInode{ino}` | its bit is set and it was not reached |
| `UnmarkedInode{ino}` | it was reached and its bit is clear |
| `UnmarkedBlock{block}` | a claimed block, or one before `data_start`, is marked free |
| `LeakedBlock{block}` | a block from `data_start` on is marked used and nothing claims it |

Only inode numbers 1 to `inode_count` and blocks below `total_blocks` are checked against the bitmaps.

`inodes` is the number of reached inodes, `blocks` the number of **different** claimed blocks. Only device errors (and a poisoned filesystem) make `fsck` return `Err`. Damage is a problem in the report, never an `Err`.

## Repair

With `repair` false, nothing is written. With `repair` true, the report lists what was found **before** repairing, and `repaired` counts the problems fixed.

- `UnmarkedInode` and `UnmarkedBlock` are always fixed: marking something as used cannot lose anything.
- `LinkCount` is set to `found` (if it fits in 16 bits), `OrphanInode` is freed (blank record, clear bit) and `LeakedBlock` is cleared. **But only if the walk was complete.** If the report has any `BadDirectory`, `BadEntry` or `BadBlockTree`, then something could not be followed. What looks unused may be in use, so **none of these three are done**.
- Nothing else is changed.

Repairs are made through **transactions**, so that a power cut during a repair adds no new damage. There may be more fixes than one transaction can hold, so use as many as you need.

## What the tests do

They plant one kind of damage at a time and expect exact reports. Then they damage random bytes: every public call must return, and **every file that could be read before a repair must read the same after it**.

**Limits worth knowing: repair can free data that was alive.** Only the superblock and the journal carry checksums. If damage makes a directory **shorter** (a smaller size, still a multiple of 64), or **zeroes an entry**, nothing looks broken. The walk counts as complete. The files that were named there look like orphans, so repair frees them and their blocks.

In the same way, a directory whose kind byte says "file" hides its children. The tests' promise about files that stay readable covers only what could still be reached by name. A real filesystem would move orphans to `lost+found`, or check each directory with a checksum.
