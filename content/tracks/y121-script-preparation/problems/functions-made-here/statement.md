A spreadsheet lets power users define their own functions inside a formula. The formulas arrive as trees. Write `run(program)`.

The `Expr` type in the starter describes each kind of node. A value is a **number** or a **function**. The whole program must give a number.

**Variables.**

- `Let(name, value, body)` evaluates `value`, then makes a **new** variable for `body`. It hides an older one with the same name there, and nowhere else.
- `Set(name, v)` changes the nearest variable with that name. It does not create one.

**Functions.**

- A function sees the variables that surround the place where it was **made**, not the place where it is called.
- It does not get copies. When `Set` changes such a variable, from inside the function or from outside, both sides see the change.
- It keeps working after the `Let` or the call that made its variables has finished.
- Each time a node runs, it makes new variables and new functions. Two functions made by two calls do not share the called function's variables.
- A parameter is a new variable in each call.
- `Recursive(name, params, body, rest)` makes a function that can use `name` inside its own body. `name` is also usable in `rest`.

**Order and errors.** Operands, then arguments, are evaluated from left to right. For `Set`, the value is evaluated before the name is looked up. The first error ends the run.

| Error | When |
|---|---|
| `Unknown(name)` | no such variable, for reading or for `Set` |
| `NotANumber` | a function where a number is needed: `Add`, `Less`, the test of `If`, or the final result. `Add` and `Less` evaluate **both** operands before checking either |
| `NotAFunction` | calling a number. Checked after all arguments have been evaluated |
| `WrongCount` | the wrong number of arguments; checked after `NotAFunction` |
| `TooDeep` | a call that would be the 201st active at once (`MAX_CALLS` is 200); checked after `WrongCount` |

`Add` wraps on overflow. The tests run `run` on a thread with a 64 MB stack.
