`Round` holds two `&mut` borrowed from its caller, and none of its methods compiles. Fix them. After
`finish`, the caller must be able to keep using the players for as long as the original borrow lasts,
even once the `Round` itself is gone. Transferring from a player to the same player is allowed (it
moves nothing but is logged).

`transfer` never needs two `&mut` at once, so don't split the slice (`split_at_mut`, `get_disjoint_mut`)
and don't move values out (`swap`, `take`).
