A rules engine stores small formulas. Two users often write the same formula with different variable names. The engine should spot this and keep one copy.

A formula is a `Term`:

- `Name(x)` uses a name.
- `Fn(x, body)` is a function with the parameter `x`. Inside `body`, `x` means that parameter.
- `Apply(f, a)` applies `f` to `a`.
- `Let(x, value, body)` gives a name to a value. `x` means that value **only inside `body`**. Inside `value`, an `x` still means whatever it meant outside.

A use of a name refers to the **nearest** enclosing `Fn` or `Let` that introduces it. If none does, it is an **outside name**: it refers to something the formula does not define.

**`nameless(term)`** writes the formula so that the names it introduces do not show.

| Term | Text |
|---|---|
| a use of a name introduced inside the formula | `#N` |
| a use of an outside name `x` | `x` |
| `Fn(_, body)` | `(fn BODY)` |
| `Apply(f, a)` | `(F A)` |
| `Let(_, value, body)` | `(let VALUE BODY)` |

`N` says which one is meant. Look at the places that introduce a name and **surround** this use, from the innermost outwards, and count from 0. `N` is the position of the one it refers to. 

Surrounding places count whether or not their names match. A `Let` surrounds its body, not its value.

So `Fn(x, Fn(y, Apply(Name(x), Name(y))))` is `(fn (fn (#1 #0)))`.

**`outside_names(term)`** gives the outside names, sorted, each once.

**`same_meaning(a, b)`** is true when the two differ at most in the names they choose for what they introduce. Outside names must be the same.

Names are non-empty and consist of lowercase letters and digits, starting with a letter. A term is at most 300 levels deep.
