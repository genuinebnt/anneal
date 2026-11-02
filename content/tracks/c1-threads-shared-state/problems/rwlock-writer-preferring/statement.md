Write `RwLock<T>` on a `Mutex` + `Condvar`. `read()` gives a shared guard (`Deref`), `write()` an exclusive one
(`Deref` + `DerefMut`); any number of readers may hold the lock together, a writer holds it alone.

**Writer preference:** as soon as a writer is *waiting*, new readers must wait too (`try_read` fails), so readers
that keep arriving cannot starve it. Readers already inside finish normally. `pending_writers()` counts writers
queued in `write`. `try_read`/`try_write` never wait. Choose the bounds for `Send`/`Sync` carefully.
