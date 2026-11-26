Three exported macros work in the author's own examples and misbehave in other people's code. Fix them. They must stay
`macro_rules!` macros with these names.

**`max_of!(e1, e2, ...)`** is the largest of one or more values of any type with `PartialOrd` (floats included, and
non-`Copy` types such as `String`).
- Every argument is evaluated **exactly once**, left to right.
- Among equal largest values the **first** wins. A value is "larger" only if `>` says so.
- A trailing comma is allowed.

**`map!{k1 => v1, k2 => v2, ...}`** is a `std::collections::HashMap`.
- It may be empty. A trailing comma is allowed. A repeated key keeps the **last** value.
- Each key and value is evaluated once, in written order (key before value).
- It must work in a module that imports nothing, and in one that defines its own type called `HashMap`.

**`ensure!(condition, "format string", args...)`** returns early from the enclosing function with
`Err(Failure(message))` when the condition is false.
- The condition is evaluated once. The format arguments are evaluated **only when it fails**.
- It must work with nothing imported besides the macro, even where the caller has their own `Result` alias or their own
  items called `Err` or `Failure`.
- A trailing comma after the last argument is allowed.
