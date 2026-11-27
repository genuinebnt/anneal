**Capstone, step 7 of 7.** After a week there are thousands of small batches, and every question visits each one. Merge them.

The starter holds the reference code of steps 1 to 6.

**Spoiler warning.** That code solves the earlier steps.

- `merge(first, count)` replaces the `count` batches starting at batch `first` (from 0, oldest first) with **one** batch that holds their pages. It returns `true`. If `count` is below 2, or some of those batches do not exist, it changes nothing and returns `false`. Any `usize` values may be passed.
- `merge_all()` leaves one batch. With none or one, it does nothing.

**A merge changes only** `batches()` and `packed_bytes()`. Every other answer stays the same: `len`, `page_len`, `postings`, `phrase`, `find_pattern` (its candidates too), `matches` and `search`, whose scores must be exactly equal.

After a merge, everything is as if those pages had been **added as one batch**. This includes `packed_bytes()`. More batches can be added, and merged again.

A merge works from what the batches keep. It is not given the pages again.

**One call, little repeated work.** A test builds 1 000 batches of two short pages that share their words. It counts the bytes that one call of `merge_all` asks the allocator for. They must be at most **2 000 times the bytes of page text**.

That leaves plenty of room to handle each page a few times, or even ten times over. It is far too little to handle the first page again for every later batch. Time is not graded.
