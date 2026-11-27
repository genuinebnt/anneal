This is step 2 of 5 of a small SQL engine that keeps its tables in memory. The engine receives text typed by strangers. This step turns **one statement** into a tree, or says exactly where the text goes wrong.

Write `parse(sql)`. The tree types are in the starter. Later steps work on these trees.

**Tokens are given.** The starter holds **step 1's reference code**, `lex`, which gives that step away, so solve step 1 first. It returns each token with its bytes, with words in small letters and `!=` as `<>`. Keywords and names may therefore use any capitals. Texts are returned as written.

**Reserved words** are never names: `select from where order by limit group and or not insert into values create table asc desc is null true false`. Every other word is an ordinary name, `int`, `count` and `sum` included.

**Statements.** `[ ]` marks an optional part, and `{ }` means "any number of times":

```text
statement := ( create | insert | select ) [ ";" ] end
create    := CREATE TABLE name "(" name type { "," name type } ")"
type      := INT | TEXT | BOOL
insert    := INSERT INTO name VALUES tuple { "," tuple }
tuple     := "(" expression { "," expression } ")"
select    := SELECT item { "," item } FROM name
             [ WHERE expression ] [ GROUP BY name ]
             [ ORDER BY name [ ASC | DESC ] ] [ LIMIT number ]
item      := "*" | aggregate "(" ( "*" | name ) ")" | expression
aggregate := COUNT | SUM | MIN | MAX
```

An item is an aggregate only if it **starts** with one of those four words followed directly by the token `(`. Nothing may follow its `)` inside the same item. This step accepts `*` in all four aggregates.

An `Item::Expr` has a `name`, used as the column title. For a plain column, brackets or not, it is the column's name. Otherwise it is the item's source text, from its first token to its last, exactly as typed.

**Expressions.** From the weakest row to the strongest:

| Operators | Notes |
|---|---|
| `OR` | groups from the left |
| `AND` | groups from the left |
| `NOT x` | may be repeated: `NOT NOT x` |
| `=` `<>` `<` `<=` `>` `>=`, `x IS NULL`, `x IS NOT NULL` | one level, grouped from the left: `a = b IS NULL` asks whether `a = b` is missing |
| `+` `-` | from the left |
| `*` `/` | from the left |
| `- x` | may be repeated |
| number, text, `TRUE`, `FALSE`, `NULL`, name, `( expression )` | |

`NOT` is weaker than a comparison. So it may only stand where a whole expression starts, or as an operand of `OR`, `AND` or `NOT`. `a = NOT b` and `- NOT b` are mistakes. A minus sign before a number is the operator, not part of the number.

**Mistakes.** The result is the first mistake, with the bytes `start..end` it covers.

The whole text is cut into tokens before any grammar is checked. So an error from `lex`, anywhere in the text, wins over a grammar mistake: return it as it is.

| `problem` | Meaning | Covers |
|---|---|---|
| `BadCharacter` | a character that starts no token (a lone `!` too) | that character |
| `UnclosedText` | a text without its closing quote | from the quote to the end of the input |
| `NumberTooLarge` | a number that does not fit | its digits |
| `Unexpected` | the first token that cannot continue any correct statement | that token, or `len..len` if the input stops too early |
| `TooDeep` | see below | any range inside the input (not tested) |

**Limits.** A tree is as tall as its longest path of nodes: a literal or a column is 1, and every operator adds 1 above its tallest operand. A tree taller than `MAX_HEIGHT` (100) is `TooDeep`. So is an expression with more than `MAX_OPEN` (100) brackets open at once. The brackets of tuples and aggregates do not count.

Hostile input has 100 000 brackets or operators and must not crash your program. Check the height while you build the tree: even throwing away a very tall tree can crash. Trees of exactly these sizes must parse.
