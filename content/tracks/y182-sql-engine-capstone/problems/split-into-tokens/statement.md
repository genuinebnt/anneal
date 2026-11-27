This is step 1 of 5 of a small SQL engine that keeps its tables in memory. The engine receives text typed by strangers. This step cuts that text into **tokens**, each with the bytes it covers.

Write `lex(sql)`. It returns the tokens in order, each as `(token, start, end)`, where `start..end` are the bytes of the source it covers.

**Tokens.** ASCII white space separates them and is otherwise ignored.

- A **word** is `[A-Za-z_][A-Za-z0-9_]*`. It is returned in small letters. Keywords are ordinary words here.
- A **number** is a run of ASCII digits. It must fit in an `i64`.
- A **text** is written in single quotes. Inside it, `''` stands for one quote. It may hold any character, and is returned as written. Its range includes both quotes.
- The **symbols** are `<=` `>=` `<>` `!=` `(` `)` `,` `*` `+` `-` `/` `=` `<` `>` `;`. Where two could start, the longer one wins. `!=` is returned as `<>`.

Tokens need no space between them: `a<=-1` is four tokens, and `1a` is a number and then a word. A minus sign is always a symbol of its own.

**Mistakes.** Return the first one, reading from the left.

| `problem` | Meaning | Covers |
|---|---|---|
| `BadCharacter` | a character that starts no token (a lone `!` too) | that character |
| `UnclosedText` | a text without its closing quote | from the quote to the end of the input |
| `NumberTooLarge` | a number that does not fit | its digits |

The input may be megabytes long, and a text may hold very many quotes. Nothing may panic.
