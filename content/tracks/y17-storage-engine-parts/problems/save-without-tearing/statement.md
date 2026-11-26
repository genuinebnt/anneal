A program keeps its whole state in one blob and saves it often. It overwrites the file in place. Last week the machine lost power and the program started with an empty state.

Write the two functions it needs, against a directory given by the trait `Disk`. The tests cut the power after every call, and check every disk that could be left behind. What a power cut can leave is the model of *What is on the disk after a power cut*: bytes are safe after `sync_file`, names after `sync_dir`, name changes arrive in order, and unsynced bytes may be cut anywhere.

**Files.** Version `n` is stored under the name `state-n`, with `n` written as a plain decimal number from 1 up. While it is being made, it may only be called `state-n.tmp`. The directory may hold other files. A name that is not exactly of these two forms (`state-07` is not) is somebody else's: leave it alone.

**`save(disk, bytes)`** stores `bytes` as the version after the highest `state-n` present, and returns its number.

- If it returns `Ok(n)`, no later power cut may lose version `n`.
- If the power fails during it, or any call fails, a restart finds the earlier state or the new one, **whole**.
- Once the new version is safe, older versions are removed.
- From the moment it is safe, the save has succeeded: a later failure, as while removing, does **not** make it fail.
- Any failure before that is returned as an error.

**`recover(disk)`** returns the highest version and its bytes, or `None`. It removes leftover `state-n.tmp` files. What it returns must be safe: a later power cut cannot make a restart return something older. If any call it makes fails, it returns that error.

**`DirDisk`** is the real directory. Implement it with the standard library. `open` creates the directory and its parents. Writing to or syncing a file that does not exist is an error. `list` returns files only, not folders.

The tests of this problem contain a working version of that power-cut model. Solve that problem first if you have not.
