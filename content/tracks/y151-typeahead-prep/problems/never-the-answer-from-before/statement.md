A timetable service works out answers that are slow to build, such as "next trains from Ely". It keeps each answer for the next visitor. When a train is moved, the stored answers that mention it are wrong and must go.

The catch is time. An answer may still be **in the making** while the train is moved.

`Shelf<K, V>` is shared by many threads, so every method takes `&self`.

- `get_or_work_out(key, work)` returns `(answer, from_shelf)`. If the shelf holds an answer for `key`, return a copy and `true`, without calling `work`. Otherwise call `work()` once and return its result and `false`.
- `changed(touches)` is called **after** the facts have changed. `touches(key)` says whether the change can alter the answer for `key`.

**What the shelf must hold.**

- When `changed` returns, no stored answer for a touched key remains.
- Stored answers for keys that are **not** touched stay.
- A result of `work` must **not** be stored if some call of `changed` that touches its key came after that `work` began and before its result was stored. Its caller still gets it.
- A change may also land at the very end, while a result is being stored. The same rule holds. No test can force that instant, so check it by reading your code.
- Every other result of `work` **must** be stored. A change that does not touch the key is no reason to drop it.
- When several results for one key may be stored, the one stored last stays.

**While `work` runs, the shelf is free.** `work` may call `changed` or `get_or_work_out` on the same shelf, for any key. Other threads may too, and must not wait for it. In particular, two threads may work out the same key at once.

`touches` must not call the shelf. If `work` panics, the panic goes on to the caller, nothing is stored, and the shelf keeps working.

The shelf has no size limit here. Speed and memory are not graded.
