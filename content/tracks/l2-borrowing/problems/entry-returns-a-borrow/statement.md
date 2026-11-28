Write the methods of `Index`, an inverted index from words to the documents that contain them, plus a
count of lookups per word. Two methods must not allocate for a word they've seen before; a test
counts allocations. `docs_mut` returns a borrow into the map that the caller edits.
