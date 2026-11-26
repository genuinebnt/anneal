> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 5. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 7 of 7.** You are handed another team's `find` and asked whether it is right. No fixed list of examples will do, because you do not know where its mistake is. Make the computer look.

Write `find_difference(candidate)`. `candidate(pattern, text)` is some engine's `find`, with the meaning of step 5. The trusted answer is the one in your starter: `reference_find`, or `Regex`. The tests use the starter's types and `parse`, so leave that code as it is.

- Return `Some((pattern, text))` where the candidate's answer differs from the trusted one. The tests check the pair themselves.
- Return `None` if you find no difference. A correct candidate must give `None`.
- Call `candidate` at most `MAX_CALLS` (20 000) times, and only with patterns that parse, of at most `MAX_PATTERN` (60) characters, and texts of at most `MAX_TEXT` (12) characters.
- **Search widely.** Against a correct candidate you must make at least 1 000 calls with at least 300 different patterns.
- **Be repeatable.** The same candidate must get the same calls and the same result every time. Do not use the clock or any crate.

The tests hand you engines that are each wrong in **one** way. Each is wrong only on some inputs: a particular part of the pattern syntax, a particular kind of character, or a particular shape of the text. Think about what could go wrong in steps 1 to 5, then make sure your inputs reach all of it.
