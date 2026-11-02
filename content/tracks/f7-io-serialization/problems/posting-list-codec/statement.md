Search engines (Lucene, tantivy, Elasticsearch) store, for every term, the sorted list of documents that contain it: a
**posting list**. Implement the codec for one list of `u32` ids in non-decreasing order.

Layout: the number of ids as a LEB128 varint; then one **block** for each group of up to 128 ids:
a width byte `w`, then that block's deltas packed `w` bits each, **least-significant bit first** (delta `i` occupies bits
`i*w .. (i+1)*w` of the block's bit stream, byte 0 holding bits 0 to 7), the block padded with zero bits to a whole number
of bytes. A delta is `id - previous id`; the previous id of the first id is 0, and for the first id of a later block it is
the **last id of the previous block**. `w` is the bit length of the block's largest delta (0 when all are 0, and then there
are no payload bytes).

`encode` fails with `NotSorted` if ids ever decrease (equal ids are allowed). `decode` fails with `Truncated` if the bytes
run out, `BadWidth(w)` if a block declares `w > 32`, `Overflow` if the running sum passes `u32::MAX`, and `TrailingBytes` if
bytes remain after the last block.
