A service is being redeployed. It must stop taking new work, let the work already accepted finish, and only then exit. If one piece of work is accepted just as the service decides it is idle, it is cut off half way.

Model this as a `Door`. Clones of a `Door` are handles to the **same** door.

- `enter()` gives a `Visit` while the door is open, and `None` once it is closed. A visit lasts until the `Visit` is dropped, however that happens, a panic included. A `Visit` is `Send`, borrows nothing and may outlive every `Door` handle.
- `inside()` is the number of visits that have not been dropped.
- `close()` closes the door for good. It returns at once. Visits already given are not affected.
- `is_closed()` says whether the door has been closed.
- `shutdown().await` closes the door, then waits until nobody is inside. It returns the number of visits the door **ever gave**. Refused calls are not counted.

**The guarantee.** It holds with any number of threads calling these at once. When `shutdown` returns, every visit the door has given **or ever will give** has already been dropped. The number returned is final.

**Waiting.**

- Any number of callers may wait in `shutdown`, together or one after another. Each returns only when the door is empty. Each gets the same number.
- With nobody inside, `shutdown` returns at once.
- Dropping a `shutdown` future before it finishes, for example when a time limit expires, leaves the door **closed**. Visits are unaffected, and a later `shutdown` waits for them as usual.
- The `shutdown` future is `Send`, so it can be spawned.
- A waiting `shutdown` must not spin. It is woken when something changes.
