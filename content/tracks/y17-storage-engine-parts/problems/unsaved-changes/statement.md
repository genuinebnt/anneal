Editors change products in a shared catalogue and save at the end of the day. Until then, their screens must show the catalogue **as it will look after saving**. The catalogue is large and far away: you can only ask it for one entry at a time.

Write `Draft`, which holds the unsaved changes. It never writes to the catalogue.

- `set(key, value)` and `delete(key)` record a change. A later change to the same key replaces it. Deleting a key the catalogue does not have is still a change.
- `revert(key)` forgets the change to `key`. The catalogue's own entry shows again.
- `get(key)` is the value after saving. For a changed key it makes **no** catalogue call, and otherwise one.
- `page(from, limit)` is the first `limit` entries after saving with `key >= from`, in key order. The page is short only when there is nothing more.
- `changes()` lists every recorded change in key order, with `None` for a delete. A change is listed even if it matches the catalogue.

**Cost of a page.** Let `n` be the number of catalogue entries from `from` up to the last key you return, or all of them from `from` on if the page is short. `page` may make at most `n + 1` calls, and none when `limit` is 0. Unsaved changes beyond the page must not be walked through one by one.
