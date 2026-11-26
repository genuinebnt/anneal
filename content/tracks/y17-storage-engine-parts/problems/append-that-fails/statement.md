A service keeps a log of records in a file. One day the disk filled up in the middle of a write. The call returned an error, the program carried on, and later writes succeeded. After the next restart, every one of those later records was gone.

Make the log safe against a failing disk. The device is behind the trait `LogDevice`, so that failures can be scripted. A failed `append` may have added **any prefix** of the bytes.

**`FileDevice`** is the real device. `open` creates the file if it is missing and keeps what it holds. Every `append` goes to the current end of the file, also after a `truncate`. `sync` makes the data durable.

**`Log::new(device)`** takes a device whose current contents are whole records. It does not write, truncate or sync.

**`Log::append(record)`** writes a 4-byte little-endian length and then the record, with **one** call to the device's `append`, and syncs before it returns `Ok`.

- If the device's `append` fails, put the device back to what it held before this call, trying once. If that works, return `NotAdded` with the append error's kind. The log stays usable.
- If it cannot be put back, return `Broke` with the **append** error's kind.
- If `sync` fails, return `Broke` with the sync error's kind. After a failed sync the operating system may have thrown the unsaved bytes away, and a later sync can still report success. So make **no further call** on the device, not even to undo.
- After `Broke`, every call returns `Unusable` without touching the device.

What must always hold: each record that got `Ok` can be read back, in order, from the front of the device. While the log is usable, nothing else is on the device.
