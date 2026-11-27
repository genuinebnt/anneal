> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 2. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 3 of 9.** Before anything runs, decide which variable each name means. Also refuse programs that parse but make no sense.

Write `resolve(program)`. It fills in `depth` on the names and returns diagnostics. `front_end`, in the starter, runs it only when parsing succeeded.

**Scopes.** A block, a function body and a class body open scopes. The top level is not one of them: it is **global**.

- A function's parameters and body share one scope.
- `for` was rewritten in step 2, so its loop variable is in the block around the `while`. The loop's step belongs to the loop, outside the body's own block.
- A class with a superclass opens a scope holding `super`. Every class then opens a scope inside it holding `this`. Each method is a function inside that one.
- The superclass name is looked up before either of those scopes opens.

**Meaning is fixed by the text.** A use of a name means the nearest enclosing declaration **written before it**. A declaration written later does not count, even if it would have run by then.

**`depth`.** For a local, the depth of a use is the number of scopes from the use out to the declaring one: 0 for the same scope. A global stays `None`.

- Set it on every use: `Var`, `Assign`, `This`, the `super` name of `Super`, and a superclass name.
- Also set `Some(0)` on every **local** declaration: variables, functions, classes and parameters. Global declarations stay `None`.
- A function or class can be named inside its own body.

**Errors** use `At::Token` and the span of the name or keyword named here.

| Message | Where |
|---|---|
| `Can't read local variable in its own initializer.` | the name read, in `var a = a;` inside a scope |
| `Can't assign to local variable in its own initializer.` | the name written, in `var a = (a = 1);` inside a scope |
| `Already a variable with this name in this scope.` | the second declaration of a **local** name; globals may repeat |
| `Can't return from top-level code.` | `return` |
| `Can't return a value from an initializer.` | `return`, with a value, directly in a method `init` |
| `Can't use 'break' outside of a loop.` | `break` |
| `Can't use 'continue' outside of a loop.` | `continue` |
| `Can't use 'this' outside of a class.` | `this` |
| `Can't use 'super' outside of a class.` | `super` |
| `Can't use 'super' in a class with no superclass.` | `super` |
| `A class can't inherit from itself.` | the superclass name |

A loop does not reach into a function declared inside it: `break` there is outside a loop. A class or function nested in another has its own rules, and the outer ones come back after it.

**Warnings.** A local declared with `var` and never **read** gets `Local variable is never used.`, at its declaration, with `warn: true`. Writing to it is not reading. Parameters, functions and classes get no warning.

Return the diagnostics in any order: `front_end` sorts them. When there is an error, `front_end` returns only the errors.
