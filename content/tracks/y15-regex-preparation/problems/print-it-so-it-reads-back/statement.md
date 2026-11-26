Feature flags are switched on by small formulas such as `a&!(b|c)`. You have the tree form. To test the code that reads formulas, you need to write trees as text, and to make up many trees.

The text is read by this grammar. There are no spaces.

```text
or    := and ('|' and)*
and   := not ('&' not)*
not   := '!' not | atom
atom  := 'a' | 'b' | 'c' | '(' or ')'
```

When the reader finds two or more parts joined by `|`, it builds **one** `Or` with all of them as a list. The same holds for `&` and `And`. A single part is just itself. Brackets only group: `a&(b&c)` is an `And` of two parts, the second an `And`.

**`to_text(formula)`** returns text that the reader turns back into exactly the same tree, using **no pair of brackets that could be left out**. Every `And` and `Or` you are given has at least two parts.

**`sample(seed, max_height)`** makes up a formula.

- A letter has height 1. Anything else has height 1 more than its tallest part. The result's height is at most `max_height`, which is at least 1.
- Every `And` and `Or` has 2 to 4 parts, and letters are `a`, `b` or `c`.
- The same arguments always give the same formula.
- Any seed works, `u64::MAX` included.

**Coverage.** The tests check that many seeds really explore the shapes. With `max_height` 4, the seeds `0..2000` must between them give:

- at least 500 different formulas, with every form at the top;
- every form directly inside each of `Not`, `And` and `Or`, itself included;
- all three letters, and lists of all three lengths.

Do not use any crate.
