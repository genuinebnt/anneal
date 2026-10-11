---
title: Durability: when a write is really on disk
summary: What happens between `write` returning and the bits being safe, what `flush`, `sync_data` and `sync_all` each promise, why a crash can leave a half-written page, and the three tools a storage engine builds from that: ordering (log before data), atomic replace and checksums.
minutes: 16
---
A bank transfer is committed. The database tells the client "done", and a second later the machine loses power. When it comes back, is the transfer still there? A database that answers "usually" is not a database. **Durability**, the D in ACID, is the promise that a change the system has acknowledged survives a crash, and the surprising thing about it is how many layers stand between your `write_all` call and the answer.

## The journey of a write

Follow one call, `file.write_all(b"...")`, down through the machine.

The call goes to the kernel as a `write` system call. The kernel does not talk to the disk. It copies your bytes into its **page cache**, a region of RAM it uses to hold file data, marks those pages dirty, and returns. The call has now succeeded: `write_all` returned `Ok`. Another process that reads the file will see your bytes, because it reads the same cache. But nothing has left RAM. If the power fails now, the bytes are gone, and the file on disk is as it was.

Later, on its own schedule (often within thirty seconds, sooner under memory pressure), the kernel writes the dirty pages out. It hands them to the block layer, which hands them to the device driver, which hands them to the **drive**. And the drive has its own cache. A spinning disk and most SSDs keep recently written data in a small volatile buffer so they can acknowledge writes quickly and reorder them. A write the drive has "completed" may still be in that buffer.

Only when the bytes are in the drive's non-volatile storage is the change durable, and getting there requires asking: first the kernel must push its dirty pages to the drive, then the drive must flush its cache. The calls that ask are the `fsync` family. Everything else, `write`, `BufWriter::flush`, even closing the file, does not.

| call | what it guarantees |
|---|---|
| `File::write_all` | the kernel has the bytes (other processes see them); not on disk |
| `BufWriter::flush` | the bytes left *your* buffer for the kernel; nothing about the disk |
| `File::flush` | nothing: a `File` has no user-space buffer, so this is a no-op |
| `File::sync_data` | the file's *contents* are on stable storage (`fdatasync`) |
| `File::sync_all` | contents **and metadata** such as size and timestamps (`fsync`) |
| `fsync` of the **directory** | a newly created or renamed file's directory entry is durable |

Two details save a lot of grief. The first is that `fdatasync` can skip updating metadata that does not matter for reading the data back (a modification time), which makes it cheaper; but if the file *grew*, the new size is metadata that does matter, and the kernel writes it. The second is that the guarantee depends on the platform. On Linux, `fsync` asks the device to flush its cache. On macOS plain `fsync` does *not* flush the drive's cache; the call that does is `fcntl(F_FULLFSYNC)`, and Rust's `sync_all` uses it there. Some cheap drives acknowledge a flush without doing it. Server SSDs with a capacitor that finishes the write after power loss make the question moot. This is why careful databases document which hardware they trust.

`fsync` is also slow, on the order of milliseconds on a spinning disk, and between tens and hundreds of microseconds on an SSD, depending on the drive. That cost has shaped every storage engine. If every commit paid for its own `fsync`, a database could commit at most a few thousand transactions per second. The answer is **group commit**: while one `fsync` is in flight, the commits that arrive queue up, and the next `fsync` makes all of them durable at once. Each client still waits for a sync that covers its change, but the cost is shared.

```svg
caption: The journey of one write. Only the last box survives a power failure. `write_all` stops in the kernel's page cache and `BufWriter::flush` in the kernel's too; `sync_data` and `sync_all` push the bytes on to the drive and ask it to flush its cache (on macOS that needs `F_FULLFSYNC`, which Rust's `sync_all` issues). (Animated: a byte travelling to stable storage.)
<svg viewBox="0 0 760 270" role="img" aria-label="Four stages a written byte passes through: the program's buffer, the kernel page cache, the drive cache and stable storage">
<style>
@keyframes fs-mv{0%,10%{transform:translateX(-576px)}22%,38%{transform:translateX(-384px)}50%,66%{transform:translateX(-192px)}80%,100%{transform:translateX(0)}}
.fs-dot{animation:fs-mv 12s ease-in-out infinite}
</style>
<defs><marker id="fs-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<rect class="bad" x="20" y="60" width="150" height="80" rx="4"/><text class="mid t-r" x="95" y="86">your buffer</text><text class="mid fg sm" x="95" y="106">BufWriter</text><text class="mid dim sm" x="95" y="124">lost on a crash</text>
<rect class="bad" x="212" y="60" width="150" height="80" rx="4"/><text class="mid t-r" x="287" y="86">kernel page cache</text><text class="mid fg sm" x="287" y="106">RAM, marked dirty</text><text class="mid dim sm" x="287" y="124">lost on power loss</text>
<rect class="bad" x="404" y="60" width="150" height="80" rx="4"/><text class="mid t-r" x="479" y="86">drive cache</text><text class="mid fg sm" x="479" y="106">volatile buffer</text><text class="mid dim sm" x="479" y="124">lost on power loss</text>
<rect class="live" x="596" y="60" width="150" height="80" rx="4"/><text class="mid t-g" x="671" y="86">stable storage</text><text class="mid fg sm" x="671" y="106">flash or platter</text><text class="mid t-g sm" x="671" y="124">survives</text>
<path class="ln" d="M170 100 H210" marker-end="url(#fs-a)"/><path class="ln" d="M362 100 H402" marker-end="url(#fs-a)"/><path class="ln" d="M554 100 H594" marker-end="url(#fs-a)"/>
<text class="mid dim sm" x="190" y="164">flush()</text><text class="mid dim sm" x="382" y="164">sync_data / sync_all</text><text class="mid dim sm" x="574" y="164">cache flush</text>
<text class="mid dim sm" x="190" y="180">or write_all</text><text class="mid dim sm" x="382" y="180">(else: the kernel,</text><text class="mid dim sm" x="574" y="180">(F_FULLFSYNC on macOS)</text>
<text class="mid dim sm" x="382" y="196">on its own schedule)</text>
<circle class="fs-dot hot" cx="671" cy="42" r="7"/>
<text class="t-a sm" x="20" y="240">A write that has returned Ok has only reached the second box. Acknowledge a commit after the sync, never before it.</text>
</svg>
```

## A failure `fsync` can report, and what to do with it

`fsync` can return an error: the disk is full, the device is gone, a write failed. What the error *means* has been a source of real data loss. In 2018 PostgreSQL developers found that on Linux, after a failed `fsync`, the kernel could drop the dirty pages it had been unable to write and clear the error, so that a **retry** of `fsync` returned success although the data was gone. Databases that had retried quietly lost data. The lesson, which PostgreSQL and others adopted, is that a failed `fsync` is not a transient condition to retry: the only safe response is to treat the file's contents as unknown and stop, usually by crashing and recovering from the log. Your code should never write "if sync fails, try again."

## Torn writes

Even a successful write is not atomic. A database page is 4 KiB or 8 KiB, and the disk writes in sectors of 512 bytes or 4 KiB. If the power fails while a page is being written, the file may hold half of the new page and half of the old: a **torn page**. Recovery code that trusts the page's bytes will read garbage that looks valid.

You can already guess the structure of the defences. A storage engine uses three, and a serious one uses all three.

**Order the writes.** The most important idea in this module: never change the data before you have recorded, durably, what you intend to change. Write a description of the change to a **log**, `fsync` the log, and only then modify the data page, whenever it is convenient. If the machine crashes, recovery reads the log and re-applies what the data pages missed. This is the **write-ahead log** (WAL) rule, and it is why a commit costs one sequential `fsync` of the log instead of random writes to many data pages. The buffer pool you built obeys the same ordering in miniature: a dirty page must reach the disk before its frame is reused (see the article on eviction and write-back ordering).

**Replace atomically.** For a small file such as a manifest or a configuration, you need the reader to see the old version or the new one, never half of each. Write the new contents to a temporary file in the same directory, `sync_all` it so the contents are durable, `rename` it over the old name (a rename within one filesystem is atomic on POSIX), and then `fsync` the *directory*, because the rename itself is a change to the directory and is not durable until the directory is synced. Miss the last step and, after a crash, you may find the old file again.

**Checksum.** Store a checksum with every page and verify it when reading. A torn page will almost certainly fail the check, so the engine knows to repair it from the log or a backup rather than trust it. Real systems add a further defence: PostgreSQL writes a full copy of a page to the log the first time it changes after a checkpoint (`full_page_writes`), and InnoDB writes pages to a separate doublewrite buffer first, so there is always one intact copy of a page whatever tears.

## Where a bug hides

The bugs in this area do not announce themselves, because a crash is needed to find them. Forgetting the directory sync after creating a file means the file disappears after a power cut though every write "succeeded". Syncing the data page but not the log means a commit is acknowledged that recovery cannot replay. Syncing the log in the wrong order with respect to the page write means a page can reach disk with changes the log has no record of, and recovery cannot undo them. Calling `fsync` inside a lock that every thread needs turns one slow disk into a stalled server. The defence is to write the *crash test* first: stop the program at every write boundary, restart, and check the state against a model. A simulated disk that silently drops every write after a chosen point is the usual tool for it, and you can build a small one in a test.

## Invariants: what must always be true

- **Acknowledge after the sync.** A change is reported as done only after `sync_data` or `sync_all` has returned success for it. `write_all`, `BufWriter::flush` and closing the file promise nothing about the disk.
- **A new file needs its directory synced too.** A created or renamed file is durable only when the directory entry is, so atomic replace is: write a temporary file, sync it, rename it, sync the directory.
- **A failed `fsync` is not retried as if nothing happened.** The kernel may have dropped the pages it could not write, so a later success proves nothing. Treat the failure as fatal for that data.
- **A reader can tell a torn write from a whole one.** Records carry a length and a checksum, so recovery stops at the first record that fails them.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `fsync(fd)`, `fdatasync(fd)` | `file.sync_all()`, `file.sync_data()` |
| `std::ofstream::flush()` (user-space buffer to the OS) | `BufWriter::flush()`; a plain `File` has no buffer |
| `rename(2)` | `std::fs::rename` |
| `O_DIRECT`, `O_DSYNC` flags | `OpenOptionsExt::custom_flags` on Unix |

**Port rule:** every place C++ calls `fsync` is `sync_all` or `sync_data`; forgetting the directory sync after a create or a rename is the same bug in both languages.

## Try it yourself

1. A function appends a record to the log with `write_all`, then updates the data page with `write_all`, then calls `sync_all` on both files, then returns. Name two different crash points at which it breaks the write-ahead rule, and say what recovery would find.
2. Why must the temporary file be in the *same directory* as the file it replaces? What goes wrong across two filesystems?
3. A colleague says "we `fsync` after every write, so we don't need a log." What does the colleague have wrong? Think about a transaction that changes two pages.
4. **Experiment.** Write a loop that appends 4 KiB to a file 1 000 times, in three variants: no sync, `sync_data` after each write, and one `sync_data` after every 100 writes. Predict the three times to within a factor of ten, then measure. What does the third number tell you about group commit?
5. **Kata (a week from now).** In a blank file, write `atomic_write` and a `checksum`-verified page reader against the tests below, from memory. Check the order of your four steps against this article.

## In real code

### Using it: atomic replace and a checksum

```rust test
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Replaces `path` with `data` so that a crash leaves the old file or the new one.
fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?; // the new contents are on stable storage before the rename
    fs::rename(&tmp, path)?;
    // make the rename durable too: sync the directory
    File::open(path.parent().unwrap_or(Path::new(".")))?.sync_all()?;
    Ok(())
}

/// A tiny checksum (FNV-1a) standing in for CRC32: a page with a different checksum was torn or corrupted.
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

#[test]
fn a_replaced_file_holds_the_new_contents_and_leaves_no_temp_file() {
    let dir = std::env::temp_dir().join(format!("anneal-durability-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("manifest");
    atomic_write(&path, b"v1").unwrap();
    atomic_write(&path, b"v2 longer").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"v2 longer");
    assert!(!path.with_extension("tmp").exists());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_torn_page_has_the_wrong_checksum() {
    let mut page = vec![7u8; 4096];
    let stored = checksum(&page);
    page[2048..].fill(0); // the second half never reached the disk
    assert_ne!(checksum(&page), stored, "recovery notices and repairs it from the log");
    assert_eq!(checksum(&vec![7u8; 4096]), stored);
}
```

### Using it: a log written before the data

```rust test
use std::fs::OpenOptions;
use std::io::{Read, Write};

/// Appends a record to the log and makes it durable before returning: only then may the caller change the data page.
fn log_append(log: &mut std::fs::File, record: &str) -> std::io::Result<()> {
    writeln!(log, "{record}")?;
    log.sync_data()
}

#[test]
fn replaying_the_log_rebuilds_the_state_a_crash_lost() {
    let path = std::env::temp_dir().join(format!("anneal-wal-{}", std::process::id()));
    let mut log = OpenOptions::new().create(true).append(true).open(&path).unwrap();
    for rec in ["set a=1", "set b=2", "set a=3"] {
        log_append(&mut log, rec).unwrap();
    }
    drop(log); // "crash": the data pages were never written
    let mut text = String::new();
    std::fs::File::open(&path).unwrap().read_to_string(&mut text).unwrap();
    let mut state = std::collections::BTreeMap::new();
    for line in text.lines() {
        let (k, v) = line.strip_prefix("set ").unwrap().split_once('=').unwrap();
        state.insert(k.to_string(), v.to_string());
    }
    assert_eq!(state["a"], "3");
    assert_eq!(state["b"], "2");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn flush_on_a_plain_file_is_a_no_op_and_sync_is_not() {
    let path = std::env::temp_dir().join(format!("anneal-flush-{}", std::process::id()));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"x").unwrap();
    f.flush().unwrap(); // does nothing for a File
    f.sync_all().unwrap(); // this is the durable one
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 1);
    std::fs::remove_file(&path).unwrap();
}
```

### In the exercises

- **1a-03:** the log file and `write_log`: what is appended must be what a later `read_log` returns.
- **1a-05:** `shut_down` makes both files durable (`sync_all`), and a new manager over the same files sees the log.
- **1f-02, 1f-03:** the buffer pool writes a dirty page before eviction, and `flush_page` on demand.

### Where it is used

- **PostgreSQL** (write-ahead log with `fsync`, full-page writes against torn pages), **SQLite** (journal or WAL file, `fsync` ordering documented in "Atomic Commit In SQLite"), **LMDB** (copy-on-write pages, `msync`), **RocksDB** (WAL plus `sync` option per write).
- The "Files are hard" and "Crash consistency" literature (Pillai et al., OSDI 2014) shows how often applications get the ordering wrong.
