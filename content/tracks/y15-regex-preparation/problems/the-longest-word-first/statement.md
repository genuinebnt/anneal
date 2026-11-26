A language's words are described by rules. Each rule is a small machine, given to you through the `Pattern` trait:

- `start()` is its first state.
- `step(state, c)` reads one character. `None` means the rule has failed: **no longer text** starting this way can match it.
- `accepts(state)` says whether the text read so far is a complete word of this rule. A longer text may still be a word, or not.

Write `lex(rules, text)`, which cuts the whole text into words.

- At each point, take the **longest** non-empty word that any rule accepts.
- If several rules accept that same longest word, the one **earliest in the list** wins.
- The next word starts right after it.
- A rule with `skip` set (for spaces, say) is matched like any other, but its words are left out of the result.

A `Token` gives the rule's index and the word's `start` and `end` (not included), counted in **characters**. If no rule accepts any non-empty word at some point, return `NoToken(pos)` with that point.

Texts can be hundreds of thousands of characters long. Once every rule has failed, stop reading. Otherwise a long text of short words takes far too long.
