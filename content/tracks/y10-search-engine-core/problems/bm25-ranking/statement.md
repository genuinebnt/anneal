Add ranking to a search index with **Okapi BM25** (`k1 = 1.2`, `b = 0.75`). `tokenize` splits text into lowercase terms (maximal
runs of `char::is_alphanumeric`, lowercased with `to_lowercase`). `Index::add(id, text)` indexes a document; an existing id is
**replaced**.

`search(query, k)` returns up to `k` pairs `(id, score)`, best first, ties broken by the smaller id, and only documents that
contain at least one query term. Each *distinct* query term counts once (`rust rust` scores like `rust`). With `N` documents, `n_t`
of them containing `t`, `tf` the count of `t` in the document, `len(d)` its number of terms and `avglen` the mean length of all
documents:

`score(d) = sum over query terms t of idf(t) * tf * (k1 + 1) / (tf + k1 * (1 - b + b * len(d) / avglen))`
`idf(t) = ln(1 + (N - n_t + 0.5) / (n_t + 0.5))`

An empty index, an empty query, or `k == 0` gives an empty result.
