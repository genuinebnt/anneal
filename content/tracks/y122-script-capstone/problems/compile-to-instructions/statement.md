> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 6. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 7 of 9.** Following the tree is slow. Now turn the program into a flat list of instructions and run those. The starter keeps your tree-walking interpreter (`run`): its answers are the truth.

Write `compile(program)` and `execute(script, fuel)`. This step covers what step 4 covered: no functions and no classes. A program that contains a `fun`, a `return` or anything about classes gives `Err(Unsupported)`, even if that part would never run.

**Code.** A `Proto` holds `code`, a source line for each instruction, and `consts`. `Call(n)` is how built-ins are called. The script ends with a `Return`.

**Locals live in stack slots.** A local declared in any scope is **not** looked up by name when the program runs. It is read with `GetLocal(slot)` and written with `SetLocal(slot)`. Only globals use names.

 Slots are numbered from 1 in the order the live locals were declared. A slot is free again when its scope ends, including when `break` or `continue` leaves the scope early.

**Jumps** go forwards or backwards, to an index in `code`. A jump's target is known only after the code it skips has been emitted.

**Lines.** `lines` is as long as `code`, and no entry is 0. An instruction that can fail carries the line the interpreter would report. Any other instruction carries a nearby source line.

**The machine.** `execute(script, fuel)` runs compiled code on a stack of values and returns the output and the runtime error. It sees only the `Proto`, never the tree.

- The script's slot 0 is taken before it starts; use it for any placeholder value. So the first local lives in slot 1.
- The meaning of each instruction is in the comments on `Op`.
- Each instruction costs fuel; the reference charges one unit. The error is the same message as before.
- A test that expects another ending gives at least **ten times** the fuel the reference spends. In the tests of fuel itself, the trace must name a line of the loop.
- Runtime errors, messages, lines and limits are exactly those of the tree-walking interpreter. The tests compare the two on random programs.

`run_vm`, in the starter, checks, compiles and runs, on a thread with a large stack. Programs with mistakes give an `Outcome` with `compile_errors`.
