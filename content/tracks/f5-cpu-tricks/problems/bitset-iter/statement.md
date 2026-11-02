`BitSet` stores a set of `usize` in `Vec<u64>`, one bit per possible member. It grows when you insert a large
index; `remove` and `contains` never grow it. `iter` yields members **ascending** and must find the next member with
`trailing_zeros`, clearing it with `x & (x - 1)`, rather than testing every bit.

`union_with`, `intersect_with` and `difference_with` modify `self` in place; the other set may be shorter or longer.
Two sets are **equal when they have the same members**, even if one of them carries extra empty words (it was
bigger once), so `==` cannot just compare the vectors.
