Build the interner a compiler front end uses for identifiers. `intern(s)` returns the same `Symbol` for equal
strings and a new one for a string it has not seen; symbols are **dense and first-seen ordered** (the first string
is `Symbol(0)`, the next new one `Symbol(1)`, ...). `resolve` goes back from a symbol to its text, `get` looks a string up
without creating anything, and `len` counts distinct strings.

Looking up a string that is already interned must not allocate. Strings may be empty or Unicode, and are stored exactly
as given (no trimming, no case folding).
