Build the index behind Google Code Search, GitHub's code search and `ripgrep`-style tools: a **trigram index**. A trigram is
any 3 consecutive `char`s of a document. `grams` maps each trigram to the sorted set of document ids that contain it.

- `add(id, text)` indexes a document. Adding an existing id **replaces** it: the old text's trigrams must no longer
  match. `remove(id)` deletes a document (and its trigrams) and says whether it existed.
- `candidates(query)` returns the sorted ids of documents that contain **all** trigrams of `query`, found from the index
  alone; `Some(vec![])` if some trigram is in no document; `None` if the query has fewer than 3 `char`s.
- `search(query)` returns the sorted ids of documents that really contain `query` as a substring (case-sensitive): the
  candidates, each confirmed against its text. Queries shorter than 3 `char`s fall back to scanning every document, and the
  empty query matches every document.

Work in `char`s, not bytes, so non-ASCII text works.
