**Capstone, step 5 of 7.** You can read a query and find words, phrases and pieces. Now answer the whole query.

The starter holds the reference code of steps 1 to 4.

**Spoiler warning.** That code solves the earlier steps.

Write `matches(query)`. It returns, rising and each once, the pages that satisfy the tree.

| Node | Pages |
|---|---|
| `Nothing` | none |
| `Word` | those that have the word |
| `Phrase` | those that have the phrase, as in step 2 |
| `Pattern` | those that fit, as in step 4 |
| `And(items)` | those in **every** item; with no items, every page |
| `Or(items)` | those in **at least one** item; with no items, none |
| `Not(item)` | every page that is not in `item` |

A caller may build any tree by hand, with lists of any length, but not deeper than a parsed one can be. Words in a tree are already in `tokenize` form.

**Speed is not graded**, because the tests cannot count your work here. To measure it, time `alpha beta NOT gamma` over 60 000 pages that mostly have all three words.
