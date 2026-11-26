> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 and 2. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 3 of 7.** A tree is easy to read but awkward to run. Write `compile(ast)`, which turns it into a **machine**: a list of states that link to each other by index.

| State | Meaning |
|---|---|
| `Char(class, next)` | read one character in `class`, then be in `next` |
| `Any(next)` | read any one character, then be in `next` |
| `Split(a, b)` | be in both `a` and `b`, reading nothing |
| `Start(next)` | move to `next`, reading nothing, only at the start of the text |
| `End(next)` | move to `next`, reading nothing, only at the end of the text |
| `Match` | accept |

The machine accepts a text if, beginning at `nfa.start`, there is a way to read all of it and reach `Match`.

**Rules for the result**

- `states[0]` is `Match`, and there is no other `Match`.
- Apart from it, there is exactly **one** state for each `Class`, `Any`, `Start`, `End`, `Star`, `Plus` and `Opt` node. An `Alt` with `n` branches adds `n - 1` more. Nothing else adds a state, and nothing is built twice.

A sequence or a list of branches may have hundreds of thousands of items.

The tests do not care in which order the states are listed. They follow your machine through texts and compare what it accepts with what the tree means.
