> **Spoiler before you open the editor.** The starter of this step contains reference code for step 1. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 2 of 9.** Now turn tokens into a tree, for the whole language. A script with five mistakes should report five, not one.

Write `parse_tokens(tokens)`. It returns the top-level statements and the diagnostics. The tree types, `dump` (the tree as text, which the tests compare) and `parse` are in the starter.

**Grammar.**

```text
program     := declaration*
declaration := 'class' NAME ('<' NAME)? '{' method* '}'
             | 'fun' method
             | 'var' NAME ('=' expression)? ';'
             | statement
method      := NAME '(' (NAME (',' NAME)*)? ')' '{' declaration* '}'
statement   := 'if' '(' expression ')' statement ('else' statement)?
             | 'while' '(' expression ')' statement
             | 'for' '(' (var-declaration | expression ';' | ';') expression? ';' expression? ')' statement
             | '{' declaration* '}'
             | 'return' expression? ';'  |  'break' ';'  |  'continue' ';'
             | expression ';'
expression  := target '=' expression | operators
```

An `else` belongs to the nearest `if`. A method named `init` gets `FnKind::Initializer`; other methods get `Method`, and `fun` gives `Function`.

**Operators**, loosest first. Every infix operator groups to the left.

| Operators |
|---|
| `or` |
| `and` |
| `==` `!=` |
| `<` `<=` `>` `>=` |
| `+` `-` |
| `*` `/` `%` |
| prefix `-` `!` |
| calls `f(a, b)` and properties `a.name`, which chain |

The other operands are numbers, strings, `true`, `false`, `nil`, names, `this`, `super.NAME` and `( expression )`. Brackets leave no node.

- Only a name or a property may be assigned to: `Assign` and `Set`.
- A `for` has no node of its own. `for (init; c; step) body` becomes `Block([init, While(c, body, step)])`. With no `init`, it is just the `While`. A missing condition is `true`.
- `Expr::This` holds a `Name` called `this`. `Expr::Super` holds a `Name` called `super`, with the span of that keyword.
- Spans in the tree are the ones named in the comments on the types.

**Mistakes.** `report` uses `At::End` for the `Eof` token and `At::Token` for any other. The diagnostic's span is that token's.

When something required is missing, report the token found there, with one of these messages:

```text
Expect expression.
Expect ';' after expression.
Expect ';' after variable declaration.
Expect variable name.
Expect ')' after expression.
Expect ')' after arguments.
Expect property name after '.'.
Expect '(' after 'if'.
Expect ')' after if condition.
Expect '(' after 'while'.
Expect ')' after condition.
Expect '(' after 'for'.
Expect ';' after loop initializer.
Expect ';' after loop condition.
Expect ')' after for clauses.
Expect '}' after block.
Expect ';' after 'break'.
Expect ';' after 'continue'.
Expect ';' after return value.
Expect function name.
Expect '(' after function name.
Expect parameter name.
Expect ')' after parameters.
Expect '{' before function body.
Expect class name.
Expect superclass name.
Expect '{' before class body.
Expect method name.
Expect '(' after method name.
Expect '{' before method body.
Expect '}' after class body.
Expect '.' after 'super'.
Expect superclass method name.
```

**Carrying on.** After such a mistake, abandon the current declaration and skip tokens. Skip at least one.

Stop just after a `;`, or just before `class`, `fun`, `var`, `for`, `if`, `while` or `return`, or at the end. A declaration inside a block recovers the same way, and the block goes on. The abandoned declaration is not in the result.

**Three mistakes are reported without skipping.** Parsing simply goes on:

- `Invalid assignment target.` at the `=`; the left side stands as the expression.
- `Can't have more than 255 parameters.` once, at the 256th parameter.
- `Can't have more than 255 arguments.` once, at the first token of the 256th argument.

**Depth.** Every later step walks the tree by recursion, so a stranger must not choose its depth.

- Each of these must be accepted **40 levels** deep, each kind on its own:
  - brackets, prefix operators and a long chain of one operator;
  - chained assignments, chained calls or properties, and calls inside arguments;
  - blocks, and functions inside functions;
  - chains of `if`, `else if`, `while` or `for`.
- This shape must be accepted too, with **40** brackets: `fun f(n) { if (n == 0) return 0; return (1 + (1 + ... f(n - 1) ...)); }`.
- Each single kind **100 000** levels deep must give diagnostics whose first message ends in `Too deeply nested.`, with no stack overflow.
- Between those, how you count and where you stop are yours. No test of this or a later step needs anything deeper accepted. Later steps walk the tree by recursion, so keep your limit modest. The reference allows `MAX_NESTING` (200) of its own counting.
- Statements side by side are not depth, however many. A mistake must not use up depth for what follows it.

These deep tests run on a thread with a 64 MB stack.

`parse`, in the starter, joins your diagnostics with the scanner's, sorts them by position and keeps the first `MAX_DIAGS` (100).
