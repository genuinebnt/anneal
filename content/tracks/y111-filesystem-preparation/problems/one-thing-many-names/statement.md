A photo library lets one photo appear under several names, in different albums. Editing it through one name changes it everywhere. The photo must be handed back, to be deleted, **only when its last name goes**.

Write `Shelf<T>`. `T` is any type: it cannot be assumed to be copyable.

- `add(name, item)` puts a new item on the shelf under a name. A used name is `Taken`, and the new item is dropped.
- `also(existing, new)` gives the item behind `existing` one more name. Errors, in order: `existing` is unknown (`Missing`); `new` is used (`Taken`).
- `get(name)` and `get_mut(name)` reach the item.
- `names_of(name)` is how many names the item behind `name` has, or 0 if the name is unknown.
- `len()` is the number of **items**, not names.
- `remove(name)` takes one name away. It returns `Ok(Some(item))` if that was the item's last name, and `Ok(None)` otherwise. Unknown: `Missing`.
- `rename(from, to)` changes a name. `from` must exist (`Missing`). `to` may be in use:
  - if `to` already names **the same item** (also when `to` is `from`), do nothing and return `Ok(None)`;
  - if it names another item, that item loses this name, and is returned if it was its last.

The shelf keeps no hidden copy of an item it has handed back. Any string is a name.
