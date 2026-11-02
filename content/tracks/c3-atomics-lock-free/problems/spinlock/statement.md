Write `SpinLock<T>`, a mutex that busy-waits. `lock()` returns a `Guard` that derefs to `T` and unlocks when
dropped. `try_lock()` never waits. `into_inner(self)` and `get_mut(&mut self)` reach the value without locking.

Many threads will increment a plain (non-atomic) `u64` through the lock and the total must be exact, so the
lock's ordering has to be right: everything done under one hold must be visible to the next. Mark the type `Sync`
and `Send` with the weakest bound that is still sound.
