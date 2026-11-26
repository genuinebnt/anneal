A store keeps its changes in batches called **runs**, ordered oldest first. A newer run overrides older ones. A deleted key is written as a marker, `(key, None)`. The runs pile up, so reads get slow and deleted data takes space.

**`lookup(runs, key)`**: the newest run that mentions `key` decides. The result is `None` if it holds a marker, or if no run mentions the key.

**`merge_window(runs, window)`** replaces the neighbouring runs `runs[window]` with **at most one** run in their place. The window is not empty and lies inside `runs`. Runs outside it must not change.

1. `lookup` must give the same answer for every key as before.
2. The new run is sorted by key and holds each key at most once.
3. A value which is the newest entry for its key inside the window is kept, even if an older run says the same.
4. A marker is kept **only if removing it would change what the runs up to and including the new one answer**.
5. A run with no entries is not left behind.

Runs can hold hundreds of thousands of entries. The window may hold a great many markers while the older runs are large.
