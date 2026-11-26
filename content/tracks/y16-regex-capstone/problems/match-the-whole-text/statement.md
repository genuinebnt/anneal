> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 3. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 4 of 7.** Run the machine. Write `is_match(nfa, text)`: can the machine read **all** of `text`, from `nfa.start`, and reach `Match`?

The states mean what step 3 said: `Split` moves to both targets without reading. `Start` and `End` also read nothing, but may be passed only when the position is the start, or the end, of the text.

**The promise that matters.** For every pattern and every text, the time must grow in proportion to `text length x number of states`. Patterns such as `(a*)*b` or `(a|aa)*c` make many engines take exponential time on a run of `a`s. That is a known way to take a service down. Yours must not care.

Loops that read nothing, as in `(a*)*`, must end.
