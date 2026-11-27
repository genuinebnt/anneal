**Capstone, step 5 of 8.** The bank wants balances, daily totals and its biggest spenders. These are **views**: built from the journal, and safe to throw away and build again. The starter holds working code for steps 1 and 2.

`Reports` counts journal lines in order.

- `next_pos()` is the position of the first line not counted.
- `catch_up(store)` counts every line the store has from there on.
- `from_log(lines)` is the reports for exactly these lines, which are the start of a log (`lines[i].pos == i`).
- `as_of(store, pos)` is the reports for the lines before position `pos`. `pos` may be past the end.
- Reports that counted the same lines are equal.

**What they say:**

- `balance(stream)`: what the events add up to. An unknown account has 0.
- `day(d)`: the lines with `ts / 86400 == d`. `events` counts every line.
- In a day, `deposited`, `withdrawn` and `transferred` add the amounts of `Deposited`, `Withdrawn` and `TransferredOut`.
- `reversed` adds the `delta` of every `Reversed` **made that day**, whatever day it corrects. A reversal changes no other column and no other day.
- `top_spenders(n)`: up to `n` accounts as `(name, spent)`, most first, ties by name. Every account with a line is listed, even with 0.

**Spent** is the total withdrawn and transferred out, less every withdrawal that was reversed. A reversal with a positive `delta` gave a withdrawal back. One with a negative `delta` took back a deposit, and changes nothing spent.

The lines here may be any events; the reports do not check the rules. Totals use `i128` and must not overflow. Lines of old schemas count like new ones.

There can be thousands of accounts, and the reports may catch up after every line.
