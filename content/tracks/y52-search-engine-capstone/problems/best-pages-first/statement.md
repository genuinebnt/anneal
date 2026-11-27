**Capstone, step 6 of 7.** A query may match ten thousand pages. People read the first ten.

The starter holds the reference code of steps 1 to 5.

**Spoiler warning.** That code solves the earlier steps, and the idea of *The best few of a very long stream*.

Write `search(query, k)`. A bad query gives the error of `parse_query`. Otherwise it returns at most `k` hits among the pages that **match** the query, best first.

A higher score is better. With equal scores the lower page number comes first. `k` is any `usize`.

**Scoring words.** These are the **distinct** words of `Word` and `Phrase` leaves that lie under an **even** number of `Not`s. So `NOT NOT rust` scores like `rust`. Patterns add nothing.

**Score** (BM25, with `k1 = 1.2` and `b = 0.75`). It is the sum, over the scoring words `t` that the page has, of

`idf(t) * tf * (k1 + 1) / (tf + k1 * (1 - b + b * len / mean))`

- `idf(t) = ln(1 + (N - n + 0.5) / (n + 0.5))`
- `tf` is how often the page has `t`, and `len` is the number of words on the page.
- `N` is the number of pages, `mean` their mean number of words, and `n` the number of pages that have `t`. All three cover **all batches**.

A matching page with no scoring word has score 0. Add the words in **alphabetical order** (`String`'s order), so that pages with the same counts and length get exactly the same score. The tests allow a tiny rounding difference in the value.

**Speed is not graded**, because the tests cannot count your work here. To measure it, search for a word that is on every page of 20 000 small batches.
