Shells turn `img_{small,large}.{png,jpg}` into four file names. Write `expand(pattern)`, which returns every word a pattern stands for.

**The notation**

- `{x,y,z}` is a group: choose one of the comma-separated choices. A group may have a single choice, and a choice may be empty: `a{,b}` gives `a` and `ab`, and `{}` stands for the empty word.
- Groups nest, and text and groups can follow each other in any way: `a{b,c{d,e}}f`.
- Outside every group a comma is ordinary text: `a,b` is one word.
- A backslash makes the next character ordinary, anywhere: `\{`, `\}`, `\,`, `\\`, or any other character.

**Order.** Words come out in the order of the choices, and the leftmost group changes slowest: `{a,b}{1,2}` gives `a1 a2 b1 b2`. Equal words are all kept.

**Errors.** Positions count characters, not bytes, from 0. The pattern is read from left to right and the first problem found is reported.

- `UnmatchedClose(pos)`: a `}` with no open `{`.
- `Unclosed(pos)`: the text ends inside a group. `pos` is the `{` of the innermost group still open.
- `TrailingEscape(pos)`: a backslash is the last character.
- `TooDeep(pos)`: more than `MAX_OPEN` (50) groups are open at once. `pos` is the `{` that goes over.

**Size.** A pattern may be 100 000 characters long or more, and no pattern may crash your function. If a correctly written pattern stands for more than `MAX_WORDS` (10 000) words, return `TooManyWords`. A pattern of a few hundred characters can stand for more words than any computer can hold, and your answer must still come back at once.
