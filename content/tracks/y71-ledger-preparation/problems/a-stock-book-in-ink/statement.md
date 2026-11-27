A warehouse keeps its stock book in ink. Nobody may cross a line out. When a line was a mistake, a new line says so and cancels it.

Write `StockBook`. Every accepted operation adds exactly one `Entry` to the end. Entries are numbered from 0 in the order added. A refused operation adds nothing and changes nothing.

- **`receive(item, qty)`** adds `In`. `qty` of 0 is refused with `Zero`.
- **`ship(item, qty)`** adds `Out`. It is refused with `Zero` for 0, then with `Short { have, need }` when there is less than `qty`.
- **`undo(n)`** cancels the effect of entry `n`. It adds `Undo { of: n, item, change }`. `change` is what the undo does to the stock: `-qty` for an `In`, `+qty` for an `Out`.

`undo` refuses in this order:

1. `NoSuchEntry`: there is no entry `n`.
2. `NotUndoable`: entry `n` is itself an `Undo`.
3. `AlreadyUndone`: an earlier entry already undid `n`.
4. `Short { have, need }`: the change would take **today's** stock of that item below zero.

Each successful call returns the number of the entry it added.

**`stock(item)`** is the amount on hand now, 0 for an item never seen. **`entries()`** is the whole book.

**`from_entries(entries)`** rebuilds a book from entries that some `StockBook` wrote. The result answers every later call exactly as the original would.

Quantities are `u32`, and a book never holds more than a few million entries, so stock fits in a `u64`. There can be hundreds of thousands of entries and undos, so no call may walk the whole book.
