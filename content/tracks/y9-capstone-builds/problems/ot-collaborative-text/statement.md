Implement the transformation function at the heart of OT-based editors (the family behind Google Docs and Etherpad), for
single-character edits. `Op::Insert { pos, ch, site }` inserts at a character index, `Op::Delete { pos }` removes the
character at an index, `Op::Nop` does nothing. `apply` and `apply_all` are provided.

`transform(a, b)` takes two ops made concurrently against the **same** text and returns `(a', b')` with
`apply(apply(s, a), b') == apply(apply(s, b), a')` for every text `s` both ops are valid on. Cases:

- insert vs insert: the one at the smaller index stays, the other shifts right by one; at the **same index the lower `site`
  goes first** (stays), the higher shifts;
- insert vs delete: if the insert index is `<=` the deleted index, the delete moves right by one; otherwise the insert moves
  left by one;
- delete vs delete: the one at the smaller index stays, the other index drops by one; deleting the same character twice
  becomes `Nop` on both sides;
- `Nop` transforms with anything unchanged.

`transform_lists(a, b)` lifts that to two whole sequences of ops, each made one after another against the same start text, so
that applying `a` then `b'` equals applying `b` then `a'`.
