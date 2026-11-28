> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 3. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 4 of 9.** Now make checked programs run. This step covers everything except functions and classes.

Write `interpret(program, fuel)`. It returns what was printed, and the runtime error if there was one. `run`, in the starter, does the rest, on a thread with a large stack. Define whatever value types you need; the tests only use `run`.

**Values** are nil, booleans, numbers (`f64`), strings, lists and the built-in functions. Only `nil` and `false` count as false.

`==` compares nil, booleans, numbers and strings by value. Lists and built-ins are equal only to themselves. Values of different kinds are not equal.

**Operators.**

- `+` adds two numbers or joins two strings. Otherwise: `Operands must be two numbers or two strings.`
- `- * / % < <= > >=` need two numbers: `Operands must be numbers.`
- `/` and `%` by zero are errors: `Division by zero.`
- `%` is Rust's `%` on `f64`: the result has the sign of the left side, so `-7 % 3` is `-1`.
- Prefix `-` needs a number: `Operand must be a number.` Prefix `!` gives a boolean.
- `and` and `or` evaluate the right side only if needed, and give one of the two **operands**, not a boolean.
- The operands of an operator are evaluated **left to right**, both before the operator checks them.
- Assignment gives the assigned value. The value is evaluated before the target is looked up.

**Variables.** The resolver of step 3 has filled in `depth`. A global that was never declared is `Undefined variable 'x'.`, when read or assigned. A global may be declared again. `var x;` is `nil`.

**Control flow.** `if`, `while`, `break`, `continue` and blocks behave as usual. A `continue` still runs the loop's step.

**Printed text.** `nil`, `true`, `false`; numbers as Rust's `{}` prints an `f64` (so `7`, `2.5`); strings as they are. A list is `[` its items joined by `, ` `]`. A list inside 9 others is printed as `[...]`, so the outer list is level 1 and the tenth is `[...]`. A built-in is `<native print>`.

**Built-ins** are global variables. Every call must go through the same path, `f(args)`.

| Call | Result |
|---|---|
| `print(v)` | prints `v` and a line end; gives nil |
| `len(v)` | characters of a string, or items of a list; else `len() expects a string or a list.` |
| `str(v)` | the printed text of `v` |
| `num(v)` | `v` if a number; for a string, the number it spells after trimming, if finite; else nil |
| `type(v)` | `"nil"`, `"bool"`, `"number"`, `"string"`, `"list"` or `"native"` |
| `assert(v)` / `assert(v, message)` | if `v` counts as false: `Assertion failed.` or `Assertion failed: message` |
| `list()` | a new empty list |
| `push(l, v)` | appends; gives nil |
| `pop(l)` | removes and gives the last item, or nil if empty |
| `get(l, i)` / `set(l, i, v)` | reads or replaces item `i`; `set` gives nil |

- A first argument that is not a list: `push() expects a list as its first argument.` (likewise `pop`, `get`, `set`).
- An index must be a whole number in range: `Index out of range.`
- A wrong number of arguments: `Expected 1 arguments but got 0.`, or `Expected 1 to 2 arguments but got 0.` for `assert`.
- Calling anything that is not a built-in: `Can only call functions and classes.` The callee and all arguments are evaluated first.

**Nesting.** No program in these tests nests deeper than step 2 requires your parser to accept.

**Runtime errors** stop the program. `error` is the message, a line break, and the trace. In this step the trace is the one line `[line N] in script`. What was printed before stays in `out`.

**Lines.** An operator's error is on the operator's line. A name's error is on the name's line. A call's error, including a built-in's, is on the line of the closing `)`.

**Limits.** A script must never crash or exhaust the host. These limits are runtime errors, with exactly these messages; each constant is the largest value **allowed**.

| Limit | Message |
|---|---|
| a string made by `+` or by `str`, or a value's printed text: `MAX_TEXT` bytes | `String too long.` |
| items in one list: `MAX_LIST` | `List too long.` |
| all printed output, line ends included: `MAX_OUTPUT` bytes | `Output limit exceeded.` |
| lists alive at once: `MAX_LIVE`, counted together with the other live objects of later steps | `Out of memory: too many live objects.` |

- A list may hold the same list many times, so its text can be far longer than the value. Do not build all of it first.
- A list that is no longer reachable and has been freed no longer counts. A script may create millions of temporary ones.
- Rust frees a chain of lists by recursion; the live limit is what keeps that safe. Lists that hold each other are never freed, and that is accepted. Each run counts only its own objects: what one run leaves behind must not count against the next, also when runs happen at the same time.

**Fuel.** Running a statement and evaluating an expression each cost at least one unit of `fuel`. When it runs out the error is `Fuel exhausted: step limit (N) exceeded.`, where N is `fuel`.

How you count beyond that is yours. A test that expects a program to finish, or to hit another limit, gives at least **ten times** the fuel the reference spends. The reference charges one unit for each statement run and each expression evaluated. The tests of fuel itself expect only that endless loops stop and that the trace names a line of the loop.

Functions, `return` and classes come in the next steps. No test here uses them.
