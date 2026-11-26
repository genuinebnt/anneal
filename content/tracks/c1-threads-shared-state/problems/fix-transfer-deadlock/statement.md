A bank keeps one `Mutex` per account so that unrelated transfers can run in parallel. In production it sometimes
**stops completely**, and the nightly audit occasionally finds a `total()` that is off although no money was lost.

Fix both. `try_lock` is not allowed.

- `transfer(from, to, amount)` moves `amount` atomically. It returns `Err(UnknownAccount(i))` for an index out of range
  (checking `from` first), else `Err(Insufficient)` when `from` holds less than `amount`, and then changes nothing.
  `from == to` is allowed: it succeeds if the account holds `amount`, and changes nothing.
- `total()` is the sum of all balances **at one moment**.
- Transfers that share no account must **not** wait for each other.

**The probe.** To make the bad timing happen every time, the bank calls a probe given by the tests:
`Step::HoldingFirst` while `transfer` holds its first lock and has not yet taken the second, and
`Step::Counted(i)` just after `total` has read account `i`. Keep both calls at those points. When there is no second
lock to take, call `HoldingFirst` while holding the one lock.
