# Chapter `math_science`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`prng_zoo`](../../examples/math_science/prng_zoo.rs) | intermediate | a PRNG zoo: LCG, xorshift64*, SplitMix64, PCG32, ChaCha8, and how to test them. |
| 2 | [`bigint_arithmetic`](../../examples/math_science/bigint_arithmetic.rs) | advanced | arbitrary-precision unsigned integers: limbs, Karatsuba, Knuth division, gcd. |
| 3 | [`computational_geometry`](../../examples/math_science/computational_geometry.rs) | advanced | computational geometry: exact predicates, hulls, triangulations, collision. |
| 4 | [`number_theory_primes`](../../examples/math_science/number_theory_primes.rs) | advanced | number-theory toolkit: sieves (plain/odd/wheel/segmented/linear), phi & mu, Pollard-Brent, CRT, BSGS, Tonelli-Shanks, Fibonacci doubling, Josephus, Pell, Collatz, Lucas. |
| 5 | [`numerics_and_math`](../../examples/math_science/numerics_and_math.rs) | advanced | numerical methods and floating-point wisdom, each checked against a closed form. |
| 6 | [`physics_simulation`](../../examples/math_science/physics_simulation.rs) | advanced | physics simulation: integrators, fixed timestep, cloth, collisions, N-body. |
| 7 | [`spatial_search_structures`](../../examples/math_science/spatial_search_structures.rs) | advanced | spatial search structures: k-d tree, quadtree, STR R-tree, grid hash, BK-tree, VP-tree. |
