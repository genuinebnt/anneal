> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 and 2. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 6 of 7.** To trust the engine you need a second opinion that shares none of its method. Write a matcher that works **directly on the tree and tries one way at a time**, going back when a way fails. It should be so plain that you can believe it.

- `backtrack_is_match(ast, text, limit)`: can the pattern match the whole text?
- `backtrack_find(ast, text, limit)`: the leftmost match and the longest from there, in characters, as in step 5.

**Steps.** `limit` bounds the work of one call. Count at least one step each time you start matching a node at a position, and no more than three.

When the limit is used up before the answer is certain, return `Err(StepLimit)`. Never turn "I gave up" into `false` or `None`. In `backtrack_find` one limit covers all starts together.

**How to try.**

- `backtrack_is_match` stops at the first way that works. In `*` and `+`, try one more round before trying to stop. In `?`, try the item before skipping it.
- `backtrack_find` must see every way from a start before it can name the longest end.
- A round of a repetition that reads nothing must be refused, or `(a*)*` never ends.
- Do not remember failed attempts, and do not run a set of states. This matcher is **supposed** to blow up on patterns like `(a*)*b`: the tests check that it gives up there.

In these tests, texts for this matcher are at most 60 characters long.
