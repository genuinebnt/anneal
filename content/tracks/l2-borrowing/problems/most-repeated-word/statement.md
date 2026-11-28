Return the `k` most frequent words of `text` with their counts: most frequent first, a tie going to the
word that appeared first. A word is a maximal run of alphanumeric chars. Words that differ only in ASCII
case are the same word (`Rust`, `rust`, `RUST`), reported with the spelling it first appeared with.

Every word you return is a slice of `text`, and you may not allocate per word: a test counts
allocations on a long text.
