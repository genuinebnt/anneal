When an order is paid, the shop must tell the warehouse. The shop used to save the payment and then send the message. One crash between the two, and a paid order sat unsent for three weeks.

Now the shop only writes its **log**. A **relay** reads the log and sends what needs sending. The network loses things, and the relay itself can crash.

A log entry has an `order` and a `Change`. `Paid` and `Cancelled` need a **notice**; `Placed` and `Viewed` do not. A notice's `id` is the entry's position in the log, from 0.

**`Relay`**

- `Relay::resume(from)` starts a relay that has nothing to do before position `from`. A first start is `resume(0)`.
- `run(log, wire)` reads the new entries of the log, then sends **every waiting notice once**, oldest first. A notice stops waiting only when `wire.send` returns `Acked`. `Unknown` means it may or may not have arrived. A notice that stays waiting does not hold back the others.
- The log only grows. `run` gets the whole log each time, and must not look again at entries it has read.
- `waiting()` is the number of waiting notices.
- `saved()` is the position the relay stores, to be passed to `resume` after a crash. It must be safe: resuming from it loses no notice. It must be **as high as possible**: the position of the oldest waiting notice, or, with none waiting, the number of entries read (`from`, before the first run).

A crash can happen at any time, and the stored position may be an **older** one that `saved()` returned earlier.

**`Inbox`** is the warehouse's side. `accept(notice)` returns `true` the first time it sees that notice, and `false` every time after.

Logs can have hundreds of thousands of entries.
