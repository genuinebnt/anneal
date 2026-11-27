**Capstone, step 1 of 8.** Over eight steps you build a bank ledger. It never edits a balance. It keeps every fact, survives a crash, and moves money between accounts. This step is the rules of one account.

`decide(account, command)` returns the events that follow, or the reason it refuses. It changes nothing. Here every accepted command gives exactly one event, with the same fields.

`evolve(account, event)` applies an event. It adds one to `version` and does what the event says to `balance` and `open`. It **never refuses or panics**, whatever the history holds.

Sums stop at the largest and smallest values, and so does the version. `replay(events)` applies them to a new account.

**Rules, checked in this order:**

- `Open`: `AlreadyOpen` if it is open. Every other command on an account that is not open: `NotOpen`.
- `Deposit` and `ReceiveTransfer`: `NonPositive` if the amount is 0 or less. `Overflow` if the balance would not fit.
- `Withdraw` and `TransferOut`: `NonPositive`, then `Insufficient { have, need }` if the balance is smaller than the amount. Spending everything is allowed.
- `ReceiveTransfer` has no other rule. The money has already left the sender.
- `Reverse { target }` cancels the event at version `target`:
  - `NotReversible(target)` unless that event was a `Deposited` or a `Withdrawn`;
  - `AlreadyReversed(target)` if it was reversed before;
  - then the change must be possible **today**. Taking back a deposit gives `Insufficient { have, need }` when the balance is below the deposit. Giving back a withdrawal gives `Overflow` when it would not fit.

The event is `Reversed { target, delta }`, where `delta` is what it does to the balance.

The fields of `Account` you may add are yours. Two equal histories give equal accounts.
