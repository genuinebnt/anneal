> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 8. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 9 of 9.** The last piece: functions that use locals of the functions and scopes around them. After this, the machine must agree with your tree-walking interpreter on every program without classes.

**The rule is step 5's.** A closure shares variables; it does not copy them. Two functions that use one variable see each other's writes, and so does the code that declared it. A variable lives on after its scope ends, however the scope ends: normally, by `return`, `break` or `continue`. Each run of a block makes new variables.

**Plain locals must stay plain.** Locals still live in stack slots, and `GetLocal` and `SetLocal` still copy the slot itself.

**Instructions.** Each function value now carries the variables it captured, in the order of its `Proto::captures`.

- `captures` lists, for each one, where the function **around** it finds it: `(true, slot)` is that function's local in `slot`; `(false, i)` is its own captured variable `i`.
- A variable used several levels down is passed through each function in between. One variable appears **once** in a function's list.
- `GetUpvalue(i)` and `SetUpvalue(i)` read and write captured variable `i`. `Closure(i)` builds the new function's list from `captures`.
- A captured local leaves the stack with `CloseUpvalue` instead of `Pop`. Only captured locals do.

**The machine.** `execute(script, fuel)` runs compiled code on a stack of values and returns the output and the runtime error. It sees only the `Proto`, never the tree.

- The script's slot 0 is taken before it starts; use it for any placeholder value. So the first local lives in slot 1.
- The meaning of each instruction is in the comments on `Op`.
- Each instruction costs fuel; the reference charges one unit. The error is the same message as before.
- A test that expects another ending gives at least **ten times** the fuel the reference spends. In the tests of fuel itself, the trace must name a line of the loop.
- Runtime errors, messages, lines and limits are exactly those of the tree-walking interpreter. The tests compare the two on random programs.

Classes remain unsupported. Chains of closures must end in the live-object error.
