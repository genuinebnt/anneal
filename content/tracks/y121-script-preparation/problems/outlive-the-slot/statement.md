A calculation engine keeps its working variables in a flat stack of numbers, because that is fast. A new feature lets a user's formula **hold on to** a variable. The formula may use it long after the engine has finished with that part of the stack.

Complete `Stack` and `Handle`. You may change their private fields.

**The stack.** `push`, `pop` and `truncate` change how many slots there are. `slots()` and `slots_mut()` give the slots themselves. The engine's inner loop computes directly in `slots_mut()`, so **a slot's value is whatever is in that slice**. They are provided, and `slots` must stay a `Vec<i64>`.

**Handles.** `keep(slot)` claims the **variable** that is in the slot now. `read` and `write` use it.

- While that variable's slot is on the stack, the handle and the slot are **one value**. A `write` shows in `slots()`. A change made through `slots_mut()` shows in `read`.
- When the slot is removed, by `pop` or `truncate`, the variable lives on. It keeps the value it had at that moment. `read` and `write` go on working.
- A later `push` into the same position makes a **new** variable. Old handles have nothing to do with it.
- Claiming the same variable twice gives handles to one variable, also after it has left the stack. `a.same(&b)` is true exactly then. A clone is the same variable.

`keep` of a slot that does not exist is `None`. `pop` on an empty stack is `None`. Handles are only used with the stack that made them.

No `unsafe` code.
