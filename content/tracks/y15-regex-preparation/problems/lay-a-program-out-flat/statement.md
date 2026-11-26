You have a tiny structured language, and a machine that can only run a flat list of operations. Write `compile(statement)`, and then a function that runs the result.

A statement is one of:

- `Emit(c)`: output the character `c`.
- `Seq(list)`: run the statements in order. The list may be empty.
- `If(flag, then, otherwise)`: test the flag and run one of the two.
- `While(flag, body)`: test the flag; if it is set, run the body and test again.
- `Break` leaves the nearest enclosing loop. `Continue` goes to that loop's test.

The machine has three operations, and **each one names where to go next** by index into `ops`:

- `Emit(c, next)`
- `Branch { flag, yes, no }`
- `Halt`

A run begins at `program.start` and follows the links. The tests run your program and the original statement side by side. Both get the same answer to each flag test. The emitted characters and the flags asked must agree, in order.

**Rules for the result**

- `ops[0]` is `Halt`, and no other operation is `Halt`.
- There is exactly one operation for each `Emit`, each `If` and each `While` in the source. So `ops.len()` is one more than their number. Nothing is written twice, and no operation exists only to jump.

`Break` or `Continue` that is not inside any loop gives `Err(CompileError::JumpOutsideLoop)`, even if it could never run.

A `Seq` may hold hundreds of thousands of statements.

**Then run it.** Write `execute(program, flag, max_steps)`. It follows the links from `program.start` and returns the emitted characters. Each time it performs a `Branch`, it calls `flag` once with that branch's flag, and the answer picks `yes` or `no`.

A program may loop for ever. Performing one `Emit` or one `Branch` is one step, and reaching `Halt` is free. If reaching `Halt` would need more than `max_steps` steps, return `Err(OutOfSteps)`, never part of the output. `flag` is not called for a step beyond the limit.
