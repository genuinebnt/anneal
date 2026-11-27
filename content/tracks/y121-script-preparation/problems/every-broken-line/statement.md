An operations team keeps its settings in small files like this:

```text
name = web;
ports = [80, 443, [8000, 8001]];
limits {
    clients = 100;
}
```

Today the loader stops at the first mistake. People fix one line, run it again, and find the next. Write `read(text)`, which returns **every whole item** and **every mistake**.

**The format.** The starter's `tokens` function has already cut the text into tokens, each with its line.

```text
file    := item*
item    := WORD '=' value ';'
         | WORD '{' item* '}'
value   := WORD | NUMBER | '[' (value (',' value)*)? ']'
```

A `WORD` is `Token::Word`. An item with `=` is a `Setting`; one with braces is a `Section`.

**Mistakes.** Each has the line of the token that was wrong. If the file ends too soon, use the line of the last token.

| Problem | When |
|---|---|
| `ExpectedName` | an item does not start with a word |
| `ExpectedEqualsOrBrace` | the name is followed by something else |
| `ExpectedValue` | a value is missing: after `=`, after `[` or after `,` |
| `ExpectedCommaOrBracket` | a value in a list is followed by something else |
| `ExpectedSemi` | the value of a setting is not followed by `;` |
| `StrayClose` | a `}` at the top level, where there is no section to close |
| `Unclosed` | the file ends inside a section; the line is that of the section's `{` |
| `TooDeep` | see below |

**After a mistake** in an item, drop that item, then pass over what is left of it. Start **at** the wrong token and count the braces you meet:

- stop after a `;` when no counted brace is open;
- stop after a `}` that closes the last counted brace;
- stop **before** a `}` when no counted brace is open: it closes the section you are in;
- or stop at the end of the file.

Then go on with the next item. One broken item gives exactly one mistake.

- A `StrayClose` takes just that `}` and nothing else.
- A section that is never closed is **kept**, with the items it holds. Each unclosed section gives its own `Unclosed`.
- A section with a broken item inside is kept too, without that item.

**Depth.** At most `MAX_DEPTH` (32) sections may be open one inside another, and a list may be at most 32 lists deep. Report `TooDeep` at the `{` or `[` that goes one further, and treat it like any other mistake. A file may contain hundreds of thousands of brackets or braces. That must not crash.

Return the mistakes ordered by line. Mistakes on one line stay in the order you meet them, with a section's `Unclosed` after those inside it.
