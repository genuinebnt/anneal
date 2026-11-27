A diagnostic tool decides things about a machine with rules such as "powered **and** (door closed **or** override)". Each test takes seconds, and some tests are only safe after another has passed. So the tool must ask **exactly** the tests it needs, in order.

The rules run on a tiny device that has a stack of booleans and can only move forwards. Write `compile(rule)`, which returns its program. The `Rule` and `Instr` types are in the starter.

**Which tests are asked.** The program must ask the same tests, in the same order, as this evaluation:

- `Test(n)` asks test `n` once. The same number in two places is asked twice, and the answers may differ.
- `Not(r)` evaluates `r`.
- `All(parts)` evaluates the parts from left to right and **stops at the first false one**.
- `Any(parts)` evaluates them from left to right and **stops at the first true one**.
- `Choose(c, a, b)` evaluates `c`, and then only `a` if it was true, or only `b` if not.

**The program.** The instructions run from the first one. `SkipIfFalse` looks at the top and leaves it there. The program ends when it moves past the last instruction.

At that point the stack must hold **exactly one** value, the value of the rule. No instruction may pop or look at an empty stack, and no skip may go further than just past the end.

**Size.** The program must stay in step with the rule. Count the **nodes** of the rule: every `Test`, `Not`, `All`, `Any` and `Choose`. Count the **links**: for every `All` and `Any`, the number of its parts. The program may have at most `4 × (nodes + links)` instructions.

A rule is at most 200 levels deep. An `All` or `Any` may have 100 000 parts.
