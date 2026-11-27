**Capstone, step 8 of 8.** The last step lets you trust the rest. You write an auditor that reads the journal on its own and says what is wrong. The starter holds working code for steps 1, 2 and 5.

`audit(lines, reports, drained)` returns every finding, **sorted** (`Finding` is `Ord`). Honest books give an empty list. `at` is the line's index in `lines`.

**Numbering first.** Line `i` must have `pos == i`, and the next version of its stream, from 1. At the **first** line that breaks this, return only `OutOfSequence` for it: nothing after it can be trusted.

Then, line by line, with one account per stream:

- `CommandRepeated`: this stream used this `cmd` on an earlier line. Every line counts, readable or not.
- `Unreadable`: the line does not decode. It is otherwise skipped, but it still takes its version in the account.
- `Impossible`: from the account as it stands, no command would make `decide` return exactly this event. Apply the event anyway before looking at the next line.

**Transfers.** A `TransferredIn` is honest only as an answer to an earlier `TransferredOut` at position `p`:

- with id `Reaction(p)`, on the transfer's target, with `from` its sender;
- or with id `Bounce(p)`, on its sender, with `from` its target.

The transfer number and amount must match too. Anything else is `Unexplained`, and answers nothing. A second honest answer to the same transfer is `AnsweredTwice`. One line may have several findings, for example `Impossible` and `Unexplained`.

If `drained`, every transfer with no answer is `Unanswered`, at the line that sent it.

**Reports**, when given, must equal `Reports::from_log` of the first `reports.next_pos()` lines. Give `ReportsWrong` if they do not, if that is more lines than there are, or if any line of the journal is unreadable.
