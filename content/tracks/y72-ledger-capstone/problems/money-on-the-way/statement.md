**Capstone, step 6 of 8.** A transfer changes two accounts, and one append changes only one. So the sender's account records `TransferredOut`, and a **courier** then credits the target. The starter holds working code for steps 1 to 3.

`tick(ledger)` deals with each line from `next_pos` that was in the log **when the tick began**, in order. Lines other than `TransferredOut` need nothing.

A line added while the tick runs, by anybody, may be left for a later tick. `next_pos` must never pass a `TransferredOut` that no tick has dealt with.

For a `TransferredOut { transfer, to, amount }` on stream `from`, at position `p`:

1. **Delivery** is the command `ReceiveTransfer { transfer, from, amount }` on `to`, with id `CmdId::Reaction(p)`.
2. If the rules refuse it, the money **goes back**: `ReceiveTransfer { transfer, from: to, amount }` on `from`, with id `CmdId::Bounce(p)`.
3. If the rules refuse that too, the transfer is **stuck**. Put `p` in `stuck` and go on.

`received` and `bounced` count the transfers whose delivery or return was **applied in this tick**.

**The courier may crash**, and restart from any older `next_pos`, even 0. Whatever happens, each transfer must end in **at most one** of delivery or return, and only once. That holds even if the target could accept the money by the time of the restart.

**A store failure** makes `tick` return the error. `next_pos` must not have passed the line that failed, so a later tick deals with it. Lines before it need not be repeated.

After a successful tick, `next_pos` is past every line that was there when the tick began. It never moves back. Clients only use `CmdId::Client` ids. Other callers may write to any account while the courier works.
