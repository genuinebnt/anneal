Firmware for a controller is shipped as small compiled routines. When one crashes in the field, all you get is the report. So the machine that runs them must never crash itself, and must say exactly where the program was.

Write `run(routines, entry, arguments, limits)`. The types are in the starter.

**The machine** has one stack of `i64` values for all routines.

- A call takes the top `takes` values of the **caller**. They become the callee's slots 0, 1, 2, ..., in the order they were pushed. Nothing is copied to a second place that the caller can still see.
- A routine's slots are everything on the stack from its slot 0 upwards. `Load(i)` and `Store(i)` reach slot `i`. Values pushed later are slots too.
- `Return` pops the result. Everything left in the callee's slots disappears, arguments included, and the result is pushed for the caller. A `Return` from the entry routine ends the run with that result.
- `Jump` and `JumpIfZero` name an index in the current routine.
- `Add`, `Sub` and `Mul` take the top two values; the second from the top is on the left.

The entry routine is called with `arguments`, which are first pushed onto the empty stack. Extra ones stay below its slot 0.

**A routine sees only its own slots.** It cannot pop, load or store a value below its slot 0.

| `Cause` | When |
|---|---|
| `Underflow` | an instruction needs more values than the routine has: a pop, or a call without enough arguments |
| `BadSlot` | `Load` or `Store` of a slot that does not exist. `Store` pops its value **first**, then looks for the slot |
| `BadAddress` | there is no instruction to run next: a jump out of range, or running off the end |
| `BadRoutine` | a call of a routine that does not exist (the entry too) |
| `Overflow` | arithmetic that does not fit in an `i64` |
| `Failed(code)` | `Fail(code)` runs |
| `TooManyCalls` | a call would make more than `limits.calls` routines active |
| `TooManyValues` | a push would put more than `limits.values` values on the stack |
| `OutOfSteps` | more than `limits.steps` instructions would run |

For a call, check in this order: the routine exists, the caller has the arguments, the call limit. The entry follows the same rules, with the arguments as what the caller has. A `Return` that frees room may push its result even when the stack was full.

**The trace** lists the active routines, innermost first, each with a line:

- the innermost: the line of the instruction that failed (for `OutOfSteps`, of the one that was not allowed to run);
- each caller: the line of its `Call`;
- for `BadAddress`, the line of the last instruction the routine ran, or 0 if it ran none.

A routine that could not start is not listed. If the entry cannot start, the trace is empty.

**Depth.** `limits.calls` may be a million. The tests recurse 100 000 deep on a thread with a normal stack, so a call must not use the machine's own call stack. `limits.values` may be huge: do not allocate it up front.
