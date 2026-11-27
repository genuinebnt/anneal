> **Spoiler before you open the editor.** The starter holds the reference answer to **Suggest by what starts a term**, with `Terms` renamed to `Index`.
>
> It also holds `search_key` and `slips`, the answers to **Light up what the user typed** and **How many slips of the finger?** Solve all three first. The code also gives away much of **What is hot right now?** and **The best players for any beginning of a name**.

**Capstone, step 2 of 4.** Typing `script` should find `javascript`. Typing `pyhton` should find `python`. Write the matching rule, then the search.

Adding, choosing, keys, scores and order by score stay as in step 1. `suggest` is replaced by `search`. A `Suggestion` now also says its `kind` and `dist`.

**The rule: `classify(typed_key, term_key)`.** Both are keys. Lengths are counted in characters. Take the first line that applies:

1. The term starts with the typed key: `Prefix`, 0.
2. The typed key has under 3 characters: no match.
3. The term contains the typed key: `Infix`, 0.
4. A typo. The **slips allowed** are 1 for 3 to 5 characters and 2 for more. Count the slips from the typed key to the whole term.

   Let `n` be the typed key's length. If the term is longer, also count the slips to its first `n` characters. `dist` is the smaller number, and must be at most the slips allowed.
5. A typo must also pass a second test. A **padded window** is 3 neighbouring places of a key that has two empty places added at each end. An empty place equals only an empty place.

   Count the **distinct** padded windows of the typed key (`w`), and how many of those the term has too. The term must have at least `max(2, w - 4 * allowed)`. Then it is `Typo`, `dist`.

Rule 5 is a choice, not a law: it drops some short typos that rule 4 would accept. For example, `rsut` does not find `rustacean`.

**The answers.** `search_with(typed, limit, now, judge)` returns at most `min(limit, 20)` suggestions:

- first the `Prefix` matches, by score;
- then the others, by `dist`, and by score within one `dist`. So infix matches come before typos.

**The judge.** `judge(term_key)` gives the verdict of the rule for the typed key. Your search must learn about `Infix` and `Typo` matches **only** from it. The tests pass their own judge and record its calls:

- No call when `Prefix` matches already fill the answer, or the typed key has under 3 characters.
- At most one call per term, and none for a term that starts with the typed key.
- Call it only for a term that **could** match. That is a term holding every unpadded window (3 neighbouring characters) of the typed key, or at least `max(2, w - 4 * allowed)` of its distinct padded windows.

`search` (given) calls `search_with` and judges with your `classify`.

**What is not graded.** The tests see only the judge calls. They cannot see how you find the terms that could match. Finding them without reading every term is the point, so check that yourself.
