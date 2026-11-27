**Capstone, step 1 of 7.** Over seven steps you build a full-text search engine. It starts by recording where each word is, in as few bytes as it can.

**Words.** `tokenize(text)` returns the words in order: the longest runs of `char::is_alphanumeric` characters, each lowercased with `str::to_lowercase`.

**A posting** says where one word is: a page number, and the positions of the word on that page. The first word of a page is at position 0.

`encode(list)` takes postings with pages strictly rising, each with at least one position, positions strictly rising. It writes numbers only:

1. the number of postings,
2. then for each posting: the **page gap**, the number of positions, and then one **position gap** for each position.

A page gap is the page minus the previous page. The first page counts from 0. Position gaps work the same way, and start again from 0 in each posting.

**A number** is an unsigned LEB128 `u32`: seven bits per byte, lowest bits first. The top bit of a byte is set when more bytes follow.

`encode` writes as few bytes as it can. For example 300 is `AC 02`.

So the postings "page 3 at positions 1 and 4" and "page 200 at position 0" are the bytes `02 03 02 01 03 C5 01 01 00`.

`decode(bytes)` reads such a list back. **The bytes may come from anywhere**, so it must never panic and never reserve memory because a number says so. 

A test counts the bytes you ask the allocator for: for an input of a few bytes, they must stay under 100 000. It reads from the left and returns the first problem:

- `Truncated`: the input ends where a number, or the rest of one, is needed.
- `Overflow`: a number does not fit in a `u32`, or adding a gap passes `u32::MAX`. A number fits if it takes at most five bytes and its fifth byte is at most `0x0F`.
- `NotAscending`: a page gap or position gap of 0, other than the first of its list.
- `NoPositions`: a posting with 0 positions.
- `TrailingBytes`: bytes are left after the last posting.

A number written with more bytes than needed, such as `80 00` for 0, is accepted if it fits the rule above.
