A shop imports its price list from a spreadsheet export, one line at a time. Product names hold commas and quote marks. Splitting at every comma has already mispriced a shelf of goods.

Write `fields(line)`. It returns the fields of one line, in order.

**A line** is one or more fields with a comma between each two. So `a,` has two fields, and the empty line has one. A field is one of two kinds, decided by its first character:

- **Quoted:** it starts with `"`. It ends at the next `"` that is **not** followed by another `"`. Inside it, `""` stands for one quote, and every other character, commas included, is itself. Only a comma or the end of the line may follow the closing quote.
- **Plain:** it runs to the next comma or the end. It may hold no `"`. Spaces are ordinary characters.

**Values.** A quoted field's value is its text, which may be empty. A plain field's value is its characters, or `None` when it has none: nothing was written there.

Each field also has the bytes `start..end` it covers in the line, quotes included. A missing field covers an empty range.

**Mistakes.** Return the first one from the left, with the bytes it is about:

| `problem` | When | Covers |
|---|---|---|
| `Unclosed` | a quoted field has no closing quote | from its opening quote to the end of the line |
| `AfterQuote` | something other than a comma follows a closing quote | that one character |
| `StrayQuote` | a plain field holds a quote | that quote |

A line may be megabytes long, with very many fields or quotes. Nothing may panic.
