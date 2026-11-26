A program writes its settings carefully, or so its author thinks. After a power cut the settings file is there and empty. To test such code you need to know every disk the power cut could leave behind.

Write `after_power_cut(ops)`. `ops` is everything a program did, in order, to the files of one directory before the power failed. Return the set of all directories that can be found afterwards.

**Names and files are separate things.** A file is its bytes. The directory says which name means which file. `Rename` moves a name and leaves the bytes alone. `Write` and `SyncFile` act on whatever file the name means **at that moment**. `Create` always makes a new file, even if the name was taken.

**The rules of this model** (a simplification of real file systems, and one you can rely on here):

1. `SyncFile` makes the bytes that file holds at that moment safe. It says nothing about any name.
2. `SyncDir` makes the directory's names safe as they are at that moment. It says nothing about bytes.
3. `Create`, `Rename` and `Remove` since the last `SyncDir` reach the disk **in order**: after the cut, the first `k` of them are there, for some `k` from none to all. A rename happens completely or not at all.
4. Each file that has a name after the cut holds its safe bytes, followed by **any prefix** of the bytes written after them. Files are cut independently of each other, and independently of `k`.

Every `Write`, `SyncFile`, `Rename` and `Remove` in the input names a file that exists at that moment, as the running program sees it. A rename replaces the file that had the new name. Renaming a file to its own name changes nothing.

The histories in the tests are small (at most a few thousand possible disks).
