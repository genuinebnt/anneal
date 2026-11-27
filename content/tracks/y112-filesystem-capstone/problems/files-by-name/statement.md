> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 4**. Solve those first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 5 of 9.** Inodes have no names. Names live in **directories**, and a directory is an ordinary file with fixed-size records in it.

## Directory records

A directory's content is a row of 64-byte **entries** (`ENTRY`).

| Offset | Content |
|---|---|
| 0 | the inode number, u32. **0 means the slot is unused** |
| 4 | the name's length in bytes, one byte |
| 5 | the name, UTF-8; the rest of the entry is zeros |

A name has 1 to 59 bytes (`MAX_NAME`). Every directory has the entries `.` (itself) and `..` (its parent). The root's `..` is the root. They are real entries, first and second.

- `lookup(dir, name)` is the inode behind `name`, if any.
- `add_entry(dir, name, ino)` uses the **first unused slot**, or adds one at the end. A name of 0 or more than 59 bytes is `NameTooLong`. It does not check `ino`, or whether the name is taken.
- `remove_entry(dir, name)` fills the slot with zeros. The directory does not shrink. A missing name is `NotFound`.
- `set_entry(dir, name, ino)` points an entry elsewhere. A missing name is `NotFound`.

All four return `NotDir` if `dir` is not a directory. The last three run inside a transaction.

**Given in this step's starter:** the whole layer of directory records. That is reading and parsing a directory with its damage checks, `lookup`, `add_entry`, `remove_entry`, `set_entry` and `start_directory`. It also gives `check_name` (may this name be given to something new?) and `components` (a path's parts).

You write path walking, `mkfs` and the operations on paths.

## Damage

Reading a directory returns `Corrupt` if:

- its size is not a multiple of 64;
- its size is above the size of the data area, `(total_blocks - data_start) * 4096`. Check this **before** allocating memory for it;
- a used entry has a name length of 0 or above 59, a name that is not UTF-8, or one that holds `/` or a NUL byte.

An entry may name an inode number out of range. Loading it is then `Corrupt`, as in step 3.

## Paths

A path starts with `/`, or it is `Invalid`. Empty parts are ignored, so `//a/` means `/a`.

Each part is looked up in turn, starting at inode `ROOT` = 1. `.` and `..` need no special case. A missing part is `NotFound`; looking a name up in a file is `NotDir`.

## Operations

`mkfs(dev, g)` formats and then creates the root, inode 1, with two links.

`create(path)` and `mkdir(path)` return the new inode number. Each is **one transaction**. Their checks run in this order:

1. the path starts with `/` (`Invalid`) and has a last part (`/` alone is `Invalid`);
2. the last part may be given to something new: `.`, `..` or a NUL byte is `Invalid`, then more than 59 bytes is `NameTooLong`;
3. the rest of the path leads somewhere, and it is a directory (`NotDir`);
4. the name is free (`Exists`).

A new file has one link. A new directory has two (its entry and its `.`), and its parent gains one (for the new `..`). A failed call changes nothing.

- `read(path, offset, len)`, `write(path, offset, data)` and `truncate(path, size)` work on files. On a directory they return `IsDir`.
- `stat(path)` returns the inode's fields. `blocks` counts data and pointer blocks.
- `readdir(path)` returns the names and inode numbers **without** `.` and `..`, sorted by name.

## Long writes

One transaction cannot be endless (step 8 will limit it). So `write` is cut into **pieces**, each one transaction. Let `W = min(16, journal_blocks - 10)`.

A piece starts where the last one ended. It ends at the end of the data, or on a block edge after **exactly `W` blocks** have been touched.

So each piece happens completely or not at all, but **the whole write does not**.

 If a later piece fails (say, `NoSpace`), the earlier ones stay, and the error is returned.

These checks come before any transaction. First check that the path is a file, then that the whole write ends at or before `MAX_FILE_SIZE` (`FileTooBig`, nothing written). Empty data writes nothing and takes no transaction.
