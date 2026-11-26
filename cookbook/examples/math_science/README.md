# Math and Science

Numerics, number theory, computational geometry, physics simulation, randomness.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

7 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`prng_zoo`](prng_zoo.rs) | a PRNG zoo: LCG, xorshift64*, SplitMix64, PCG32, ChaCha8, and how to test them | intermediate | reproducible simulation and property tests (seed it, log the seed), Monte Carlo, game logic, | std only |
| [`bigint_arithmetic`](bigint_arithmetic.rs) | arbitrary-precision unsigned integers: limbs, Karatsuba, Knuth division, gcd | advanced | RSA/Diffie-Hellman/elliptic-curve arithmetic (real libraries use fixed-width, constant-time | std only |
| [`computational_geometry`](computational_geometry.rs) | computational geometry: exact predicates, hulls, triangulations, collision | advanced | convex hulls for outlier/bounding shapes, point-in-polygon for map lookups, triangulation for rendering concave polygons, Delaunay meshes for terra... | std only |
| [`physics_simulation`](physics_simulation.rs) | physics simulation: integrators, fixed timestep, cloth, collisions, N-body | advanced | game engines (fixed timestep + accumulator, broad-phase grids, position-based cloth), molecular dynamics (velocity Verlet), astrophysics (tree code... | std only |
| [`numerics_and_math`](numerics_and_math.rs) | numerical methods and floating-point wisdom, each checked against a closed form | advanced | FFT for audio/image filtering and fast big-number multiplication, Newton/secant for solving equations, quadrature and ODE solvers in simulation, LU... | std only |
| [`number_theory_primes`](number_theory_primes.rs) | number-theory toolkit: sieves (plain/odd/wheel/segmented/linear), phi & mu, Pollard-Brent, CRT, BSGS, Tonelli-Shanks, Fibonacci doubling, Josephus, Pell, Collatz, Lucas | advanced | primes and factorisation in RSA/DH key generation and in attacks on weak keys; CRT inside RSA and in | std only |
| [`spatial_search_structures`](spatial_search_structures.rs) | spatial search structures: k-d tree, quadtree, STR R-tree, grid hash, BK-tree, VP-tree | advanced | GIS bounding-box queries and spatial joins in geospatial databases (R-tree family, as in PostGIS GiST | std only |
