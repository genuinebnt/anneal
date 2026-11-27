A small controller runs user scripts. It has no room for a table of variables. Its only memory is **one stack**, and an instruction can reach a value only by its slot number, counted from the bottom. Write `compile(program)`.

The `Stmt`, `Expr` and `Instr` types are in the starter. A program is a list of statements, and it is a block.

**Meaning.**

- `Let(name, value)` makes a new variable. It exists from the **next** statement to the end of its block. So `value` may use an older variable of the same name.
- An inner block may declare a name again; that hides the outer one until the block ends.
- `Assign` and `Name` mean the nearest variable with that name that exists at that point.
- The two sides of `Add` and `Sub` are worked out from left to right.
- `Block` and the body of `Loop` are blocks. Each round of a loop makes the body's variables again.
- `LeaveIf(test, n)` evaluates `test`. If it is not 0, the program goes on after the `n`th loop around the statement, counted from the innermost, which is 1. The blocks left that way end.

**Errors** are found when compiling, even in code that could never run.

| Error | When |
|---|---|
| `Unknown(name)` | no such variable exists at that point |
| `Twice(name)` | a block declares a name it has already declared itself |
| `NoSuchLoop` | `LeaveIf(_, n)` where `n` is 0 or more than the number of loops around it |

Report the first error in the order of the text. Within one statement, the expression is checked before the statement's own name or loop count.

**The stack must stay small.** At every moment of a run, the stack may hold only:

- one slot for each variable that exists at that moment, and
- the values of the expression being worked out.

A variable whose block has ended takes no slot, however the block was left. When the program ends (it moves past the last instruction), the stack must be **empty**. Arithmetic wraps.

Scripts have at most a few thousand variables, and blocks nest at most 100 deep.
