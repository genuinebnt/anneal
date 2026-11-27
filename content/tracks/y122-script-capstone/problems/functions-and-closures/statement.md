> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 4. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 5 of 9.** Add functions: declarations, calls, `return`, and functions as values.

**Calls.** `fun f(a, b) { ... }` creates a function and binds it to `f`. A function may call itself, and globals declared later, because those are looked up when it runs. Without a `return`, or with a bare `return;`, the result is nil. `return` leaves from any depth of blocks and loops inside the function.

- The callee and then each argument are evaluated, left to right, before anything is checked. Operands still go left to right, also when they are calls.
- A wrong count is `Expected 2 arguments but got 1.`

**Closures.** A function sees the variables around the place where it was **written**, not where it is called.

- It shares those variables; it does not copy them. Writes on either side are seen by the other.
- Each time a block runs, its variables are new. Functions made in different turns of a loop keep different ones. A `for` loop's own variable is **one** variable for the whole loop.
- A function can outlive the call or block that declared what it uses.

**Values.** A function is equal only to itself. `type` gives `"function"`. It prints as `<fn name>`.

**Traces.** A runtime error lists every active call, innermost first, then the script:

```text
Undefined variable 'missing'.
[line 2] in inner()
[line 5] in outer()
[line 7] in script
```

The first line number is where the error happened. Each following one is where the call above it was made: the line of that call's closing `)`. Calls that have returned are not listed. A wrong argument count is reported in the **caller's** frame.

**Depth.** `MAX_CALL_DEPTH` (200) script calls may be active at once. The next one is `Stack overflow.`, reported where the call was made, with all 200 frames in the trace. Built-ins do not count.

`run` already uses a thread with a large stack, because one script call uses several Rust calls.

**Live objects.** Every function that is created counts toward `MAX_LIVE`, like a list, until it is freed. A chain of functions that hold functions must end in `Out of memory: too many live objects.`, not in a crash.

A function declared inside a scope and that scope may hold each other. You need not free such a pair, and then it counts for the rest of the run. No test depends on it either way here.
