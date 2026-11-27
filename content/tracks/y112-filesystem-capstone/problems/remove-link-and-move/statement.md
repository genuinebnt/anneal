> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 5**. Solve those first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 6 of 9.** Names can now be added. This step takes them away, adds second names and moves them. The rule to keep: **an inode and its blocks are freed when, and only when, its last name goes.**

Each call is **one transaction**. A call that fails changes nothing.

"The parent checks" below are the first three checks of `create` in step 5. They ask for a rooted path with a usable last part, and a directory to hold it.

## `unlink(path)`

Removes a file's name. Checks, in order: the parent checks; the name exists (`NotFound`); it is not a directory (`IsDir`). If that was the file's last link, free its blocks and its inode. Otherwise lower the count.

## `rmdir(path)`

Checks, in order: the parent checks; `NotFound`; `NotDir`; the directory holds only `.` and `..` (`NotEmpty`). The parent loses a link, and the directory is freed.

## `link(existing, new)`

Gives a file one more name. Checks, in order: `existing` resolves; it is not a directory (`IsDir`); the parent checks for `new`; `new` is free (`Exists`). Directories are refused because a second name for one could close a loop. A link count cannot pass `u16::MAX` (`Invalid`).

## `rename(from, to)`

Gives `from`'s inode the name `to`. Checks, in order:

1. the parent checks for `from`, and `from` exists;
2. the parent checks for `to`;
3. if `from` is a directory: the directory that would hold `to` is **not the moved directory or anything inside it** (`Invalid`). Text can mislead here: `/ab` is not inside `/a`, and `/x/../a/b` is inside `/a`;
4. if `to` already names **the same inode**, return `Ok` and change nothing;
5. if `to` exists: a file cannot replace a directory (`IsDir`), a directory cannot replace a file (`NotDir`), and a replaced directory must be empty (`NotEmpty`).

A replaced file loses that name, as with `unlink`. A replaced directory is removed, as with `rmdir`. Then the old name goes and the new one is added. A directory that changes parent has its `..` rewritten, and the two parents' link counts change with it.

The disk may be damaged. If following `..` upwards from `to`'s directory does not reach the root within `inode_count` steps, or a `..` is missing, return `Corrupt` rather than looping. Only moving a directory needs this walk.

The tests check, after many random operations, that each link count equals what the tree holds and that no block or inode is lost.

The smallest journal, 12 blocks, is enough for any one of these calls on a small disk. Step 8 will make that limit real.
