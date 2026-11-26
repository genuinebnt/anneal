Property testing has found a list on which your code fails, but it has thousands of elements. Write `shrink(input,
fails)`, which makes it small. `fails(candidate)` says whether a candidate still triggers the failure; it is pure (the
same candidate always gives the same answer).

The result must:

1. still fail;
2. be **minimal under removal**: removing any single element gives a list that does not fail;
3. be **minimal under simplification**: replacing any single element `x != 0` by `0`, by `x / 2`, or by
   `x - x.signum()` gives a list that does not fail.

It only has to be minimal in this **local** sense. Those are the conditions on the result, not the only steps you may
try. Also:

- If `input` itself does not fail, return it unchanged.
- Elements keep their relative order; you only delete elements and move values towards zero.
- **Calls are expensive.** For an input of 10 000 elements whose failure depends on two specific elements, you may
  call `fails` at most **500** times.
- Values can be huge. Shrinking one value near `i64::MIN` must take thousands of calls at most (the tests allow 20 000
  for a two-element list), so you cannot walk there one by one.
