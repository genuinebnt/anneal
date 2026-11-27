**Capstone, step 2 of 7.** An engine cannot rebuild everything each time pages arrive. So pages come in **batches**, and each batch gets its own index, which never changes afterwards.

The starter holds the reference code of step 1. It is a stand-in, since the app cannot carry your own code over yet.

**Spoiler warning.** That code also solves *Pack where each word is*.

Pages are numbered 0, 1, 2 and so on across all batches. Write `Index`:

- `add_batch(pages)` adds these pages as **one new batch**. An empty batch changes nothing.
- `batches()` and `len()` count the batches and the pages.
- `page_len(page)` is the number of words on a page, or `None` for a page that does not exist.
- `postings(word)` returns the postings of a word over **all** batches, in page order. The word is given as `tokenize` returns it.
- `phrase(words)` returns, rising, the pages where these words stand **one straight after another**, in this order. The words are given as `tokenize` returns them. An empty phrase gives no pages. One word gives the pages that have it.
- `packed_bytes()` returns the total of `encode(list).len()` over every batch and every word in it. Here `list` holds that word's postings **in that batch**, with their global page numbers. The engine is meant to keep its postings in this packed form.

**Speed is not graded**, because the tests cannot count your work here. To measure it, time a phrase of two words that are on every one of 40 000 pages.
