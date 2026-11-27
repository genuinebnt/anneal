A library keeps one copy of a popular book. The desk has had trouble: the card says one thing and the shelf says another, and nobody knows why.

So the desk stops editing the card. It writes down **what happened**, one line at a time, and works the card out from those lines.

A **command** is something a person asks for. An **event** is something that happened. `decide` turns a command into events, or refuses it. `evolve` applies one event to the card.

**`evolve(book, event)`** records the event. It never refuses and never panics, whatever the card says. A history written under older rules must still load.

| Event | What it does to the card |
|---|---|
| `Lent { member }` | `out_to` is the member, `held_for` is empty, `lends` goes up by one (stopping at the largest value) |
| `Returned` | `out_to` is empty |
| `Queued { member }` | the member joins the back of `queue` |
| `LeftQueue { member }` | the member is removed from `queue`, every time they appear |
| `SetAside { member }` | `held_for` is the member, and they are removed from `queue` |
| `HoldDropped` | `held_for` is empty |
| `Retired` | `retired` is true, `held_for` and `queue` are emptied |

**`replay(events)`** is the card after those events, starting from `Book::default()`.

**`decide(book, command)`** returns the events in the order they happen. It does not change the book. When several refusals fit, return the first in the order written here.

- **`Lend { member }`**: `Retired` if retired. `OnLoan` if somebody has it. `HeldForAnother` if it is set aside for someone else. Otherwise `Lent`.
- **`Return { member }`**: `NotYourLoan` unless that member has it. Otherwise `Returned`. If someone is queued, follow it with `SetAside` for the first of them.
- **`Reserve { member }`**: `Retired` if retired. `AlreadyYours` if the member has it, has it set aside, or is queued. If nobody has it and it is not set aside, `SetAside`. Otherwise `Queued`.
- **`Cancel { member }`**: if it is set aside for them, `HoldDropped`, then `SetAside` for the first in the queue, if any. If they are queued, `LeftQueue`. Otherwise `NothingToCancel`.
- **`Retire`**: `Retired` if already retired. `OnLoan` if somebody has it. Otherwise `Retired`.

The refusal and the event for retiring share a name: `Refusal::Retired` and `Event::Retired`.
