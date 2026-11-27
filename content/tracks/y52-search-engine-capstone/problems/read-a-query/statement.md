**Capstone, step 3 of 7.** People type queries such as `rust (async OR "memory ordering") NOT *cast*`. This step turns the text into a tree.

The starter holds the reference code of steps 1 and 2. It also gives `fold`, which lowercases one character at a time.

**Spoiler warning.** That code solves the earlier steps.

Write `parse_query(text)`.

**Tokens.** `(`, `)`, a quote and a bare word. A **quote** runs from a `"` to the next `"`. A **bare word** is a longest run of characters that are not whitespace (`char::is_whitespace`), brackets or `"`. Only a bare word spelled exactly `AND`, `OR` or `NOT` is a keyword.

**Grammar**, from the loosest to the tightest:

- Items joined by `OR` make `Or`.
- Items written side by side, with or without `AND` between them, make `And`. So `a b OR c` is `(a AND b) OR c`.
- `NOT x` makes `Not(x)`. It applies to the one item after it, and may repeat.
- An item is a bracketed query, a quote, a bare word, or `NOT` and an item.

**Tree.** `And` and `Or` list their items in order, and always have at least two. One item alone stays as it is. Brackets add no node and are never flattened: `(a b) c` is `And([And([a, b]), c])`.

**Leaves.**

- A bare word that contains `*` is `Pattern(fold(word))`.
- Any other bare word, and any quote, goes through `tokenize`. No words give `Nothing`, one gives `Word`, and more give `Phrase`. So `don't` is a phrase, and a `*` inside quotes only separates words.

**Errors.** Positions count characters from 0. The whole text is split into tokens first.

- `UnterminatedQuote(pos)`: a quote never closes. `pos` is its opening `"`. This is found while splitting.
- `Unexpected(pos)`: a token that cannot be there, such as a leading `OR`, `AND AND`, `()` or a stray `)`.
- `UnexpectedEnd`: the text ends where more is needed: an empty query, an unclosed bracket, or a query ending in `AND`, `OR` or `NOT`.
- `TooDeep(pos)`: more than `MAX_NESTING` (50) levels are open at once. A level is opened by each `(`, until its `)`, and by each `NOT`, until its item ends. `pos` is the token that goes over.

After splitting, the first problem from the left is reported.

**Size.** A query may have hundreds of thousands of words or brackets. Nothing may crash the program, and using or dropping an accepted tree must not either.
