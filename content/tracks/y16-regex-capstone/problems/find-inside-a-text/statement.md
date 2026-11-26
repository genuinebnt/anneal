> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 4. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 5 of 7.** Wrap the engine in a `Regex` type and teach it to search.

- `Regex::new(pattern)` parses and compiles, passing on parse errors.
- `is_match(text)` is true when the pattern matches the **whole** text, as in step 4.
- `find(text)` returns the match that **starts leftmost** and, among the matches starting there, the **longest**, as `(start, end)` with `end` not included. Positions count **characters**. A match may be empty, also at the very end of the text.

`^` and `$` keep their meaning: they hold only at the start and end of the **whole** text, wherever the match begins.

Which branch of `|` is written first makes no difference: `a|ab` on `ab` finds both characters. (Perl-style engines answer differently.)

**Time.** One attempt from one start must end as soon as no longer match is possible. A long text on which every attempt fails quickly must take time in proportion to its length. No pattern may cause exponential time.
