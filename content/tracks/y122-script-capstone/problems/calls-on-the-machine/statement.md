> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 7. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 8 of 9.** Teach the compiler and the machine functions. One kind waits for step 9: a function that uses a local of a function or scope **around** it.

Unlike a tree-walker, which may keep a local function and its scope alive for ever, the machine frees a function once nothing holds it.

**Compile** `fun` and `return`. Each function becomes its own `Proto`, listed in the `protos` of the one it is written in. `Closure(i)` pushes a function value made from `protos[i]`.

- If any function body uses a local declared outside that function, return `Err(Unsupported)`. A local function that names **itself** does this too.
- Globals are fine, and so are a function's own parameters and locals.
- Classes are still unsupported.

**Calls.** `Call(n)` finds the function under its `n` arguments. The arguments become its first locals, in slots 1 to `n`, and slot 0 is the function itself. `Return` replaces all of that with the result.

- Everything behaves as in step 5: arity, the order of evaluation, nil without `return`, printing, `type`, equality and live objects.
- `MAX_CALL_DEPTH` calls may be active; the next is `Stack overflow.`
- Traces are the same, line for line.

The machine must not use Rust recursion for a script call.

**The machine.** `execute(script, fuel)` runs compiled code on a stack of values and returns the output and the runtime error. It sees only the `Proto`, never the tree.

- The script's slot 0 is taken before it starts; use it for any placeholder value. So the first local lives in slot 1.
- The meaning of each instruction is in the comments on `Op`.
- Each instruction costs fuel; the reference charges one unit. The error is the same message as before.
- A test that expects another ending gives at least **ten times** the fuel the reference spends. In the tests of fuel itself, the trace must name a line of the loop.
- Runtime errors, messages, lines and limits are exactly those of the tree-walking interpreter. The tests compare the two on random programs.
