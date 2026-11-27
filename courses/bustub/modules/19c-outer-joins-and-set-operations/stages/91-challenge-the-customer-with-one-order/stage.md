A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`partner_counts` in `src/execution/outer_join_counts.rs` answers the report "for every customer (left row), how many orders (right rows) have the same key?" as `select a.id, count(...) from a left join b on a.k = b.k group by a.id`. It uses the right outer join, so customers without orders are listed, and the report shows them with the wrong number. Find the bug and fix it.

## Why

The report is the reason outer joins exist, and the wrong number is the most common bug written with them. Nothing crashes and every customer with at least one order is right, which is why the bug survives review.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||It counted every joined row, including the NULL-padded row a customer without orders gets, so those customers showed 1. That is `count(*)`; the report needs `count(b.id)`, which skips NULLs.||

## The contract

- One `(payload, count)` per left row, in the order of the left rows. A left row's count is the number of right rows with an equal key.
- A NULL key has no partners: its count is 0, and it is still listed.
- The right payload is never read for its value, only for its presence.

## Invariants

These must hold after every step, whatever the input:

- Every left row is listed exactly once, whatever its count.
- The counts add up to the number of matching pairs of the inner join.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a right row with a key no left row has changes nothing.
- Adding a right row with the key of a left row raises that row's count by one and no other.

## Examples

Worked cases (the tests include them):

```text
left [(1, id 10), (2, id 20)], right [(1, a), (1, b)] -> (10, 2) (20, 0)
left [(NULL, id 30)], right [(NULL, a)] -> (30, 0)
```

## What the tests check

- Customers without orders have count 0.
- Customers with several orders.
- NULL keys.
- The invariants and relations on random inputs.

## Done when

All the `s3j_c2` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Modern SQL](/cookbook/p/modern-sql): COUNT(*) counts rows, COUNT(x) counts non-NULL values of x.
- [Unnesting subqueries](/cookbook/p/unnesting-subqueries): the COUNT bug: rows with no match lose their count of zero.
