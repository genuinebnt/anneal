Implement the receive side of a TCP-like stream. `Reassembler::new(isn, window)`: the stream's first byte has sequence number
`isn` (offset 0); sequence numbers are `u32` and **wrap around**, so a stream crosses `u32::MAX` back to 0.

`segment(seq, data, fin)` may be called with segments in any order, with duplicates, and with overlaps. Rules:
* Only bytes at stream offsets `[next, next + window)` are kept, where `next` is the first byte not yet handed out. Earlier bytes
  are duplicates; later ones are dropped (a real sender would retransmit them). A `fin` whose end lies beyond the window is ignored.
* Where segments overlap, the **first-arrived** bytes win.
* `read()` returns the newly contiguous bytes, each exactly once. `is_finished()` is true once a `fin` was accepted and everything before it
  was handed out. `buffered()` counts out-of-order bytes held.
