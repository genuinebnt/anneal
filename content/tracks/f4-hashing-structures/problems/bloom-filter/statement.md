Implement `Bloom::new(expected, fp)` sized for `expected` items at false-positive rate `fp`:
`m = ceil(-n ln p / (ln 2)^2)` bits and `k = round(m / n * ln 2)` hash functions (at least 1). For 1000 items at 1% that is
9586 bits and 7 hashes.

`insert` sets the k bits chosen by **double hashing** (`position_i = (h1 + i * h2) mod m`, with `h2` forced odd) and returns whether any bit
changed; `contains` is true only if all k bits are set: **no false negatives, ever**. Also provide the counters, the load-based
`estimated_fp_rate` (`(set bits / m)^k`) and `union_with` for two filters of identical shape (`Err(())` otherwise).

Generic over `T: Hash + ?Sized`, so both `&str` and `String` keys work (`insert("a")`, `contains(&String::from("a"))`).
