**Capstone, step 4 of 8.** The journal moves to a file. Money records must survive a crash, and damage in the middle of the file must never be "repaired" by deleting what follows. The starter holds working code for steps 1 to 3.

**A frame** holds one batch. All numbers are little-endian:

```
len          u32, the length of the payload
len check    u32, crc32 of the 4 bytes of len
data check   u32, crc32 of the payload
payload      the batch as JSON, a Vec<Stored>
```

`frame(batch)` makes it. `crc32` is given, and `HEADER` is 12.

**`recover(bytes)`** reads frames from the start. It returns the journal, the number of bytes in whole good frames (`keep`), and the `Tail`. At each frame, in order:

1. Fewer than 12 bytes left: the log ends, `Torn`.
2. The length check fails, or `len` is 0: `Torn` if **every byte left** is zero. Otherwise `Err(Damaged)`.
3. Fewer than `len` bytes of payload left: `Torn`.
4. The data check fails: `Torn` if **every byte after this frame** is zero. Otherwise `Err(Damaged)`.
5. The payload is not a JSON batch that `admit` accepts: `Err(BadBatch)`, wherever it is.

`at` is always the offset where that frame starts. With nothing left, the tail is `Clean`. No input may panic, and a huge `len` must not allocate.

**What these rules cannot tell apart.** A last frame whose data was damaged after it was saved looks like a torn write. Rule 4 cuts it, although its batch may have been acknowledged. No rule that reads only this file can know.

The rules also err the other way. A crash can leave zeros before the length and its check are complete. Rule 2 refuses that file, and a person must look. A cut-off or zeroed end of the file can also silently remove several batches you were told were saved.

**`FileStore::append`** works only while the store is healthy. Otherwise it returns `Poisoned` without touching the device. A refusal from `prepare` writes nothing and leaves the store healthy. So does an empty list, which makes no device call.

For a real batch, call `device.append(frame)` **once**, then `device.sync()`. The lines can be read only after **both** succeed.

If either fails, return `Io`, and the store is poisoned for good. Reads keep working.

**`FileStore::open(path)`** creates a missing file. A load error changes nothing in the file. A torn end is cut off the file. New frames go to the end.

`open` should also sync after cutting, and sync the directory when it creates the file. The tests cannot cut the power, so they cannot check those two.
