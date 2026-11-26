# Practice: math and science

How to use this page: run the recipe, read the oracle in its `main()`, then re-derive one formula on paper before touching code.
Numerical code fails quietly, so every task asks for a measurement: an observed convergence order, an error curve, an exact-versus-float comparison, or a brute-force oracle.
Order: [S] about 30 minutes, [M] about 2 hours, [A] a weekend evening or more. No solutions are given.

Rule for floats: never `assert_eq!` on `f64` results of different algorithms; compare with relative error plus an ulp bound (see `numerics_and_math::ulp_distance`) and say why the tolerance is what it is.

---

## bigint_arithmetic

**Prove you got it**
1. Why do u32 limbs with a u64 intermediate avoid needing u128, and what is the largest value `a*b + carry + limb` can reach?
2. At what limb count does Karatsuba beat schoolbook on your machine? Measure by varying the threshold constant (32 limbs in the recipe).

**Try next**
- [S] Add `to_decimal_string` and `parse` round trip; oracle: `u128` for values under 2^128 and `fibonacci(300)` known digits.
- [M] Add modular exponentiation `powmod(a, e, m)` using your divmod; oracle: `number_theory_primes::powmod` with u64 moduli.
- [A] Compute 100,000! and 2^1,000,000 digit counts, then implement the divide-and-conquer decimal conversion (the naive one is quadratic) and time it. Compare with [num-bigint](https://github.com/rust-num/num-bigint) and read how [malachite](https://github.com/mhogrefe/malachite) selects algorithms by size.

**Use in**: exact factorials, RSA toy demos (`cryptography/modular_arithmetic_dh_rsa`), decimal libraries.

**Pitfall**: normalising (stripping leading zero limbs) after every operation; one missing normalisation makes `==` and `cmp` wrong only for some inputs.

---

## computational_geometry

**Prove you got it**
1. Why is `orientation` exact for i64 coordinates computed in i128 but not for f64? Produce three f64 points where the sign is wrong.
2. For `seg_intersect`, which degenerate cases (collinear overlap, touching endpoints) does `seg_kind` classify, and how does `seg_oracle` avoid trusting the same formula?

**Try next**
- [S] Add `point_in_convex_polygon` in O(log n) and test against `ray_cast` on 10^4 random points.
- [M] Add Graham scan or Andrew's monotone chain and compare against `convex_hull` and `gift_wrap` on inputs with collinear points and duplicates (lines, grids, all-equal points).
- [A] Compute the Delaunay triangulation of 10^4 random and 10^4 clustered points, verify the empty-circle property with `incircle` for every triangle, and compare triangle count with 2n - 2 - h. Cross-check against [delaunator-rs](https://github.com/mourner/delaunator-rs) and read how [robust](https://github.com/georust/robust) and [spade](https://github.com/Stoeoef/spade) handle predicates.

**Use in**: GIS, collision, mesh generation. Real crate: [geo](https://github.com/georust/geo).

**Pitfall**: epsilon comparisons that make `orientation` non-transitive; convex hull output then loops or self-intersects.

---

## number_theory_primes

**Prove you got it**
1. Why does the segmented sieve need only primes up to sqrt(hi)? State memory usage for lo ~ 10^12 and length 10^7.
2. Why does Pollard-Brent need a primality test first (Miller-Rabin with which bases for u64)?

**Try next**
- [S] Verify the linear sieve's smallest-prime-factor array equals trial-division factorisation for all n < 10^6.
- [M] Add Euler's phi via the sieve and via factorisation; assert agreement, then use it to verify `sum_{d|n} phi(d) = n` on n < 10^5.
- [A] Factor 1,000 random 62-bit semiprimes with `factorize`; report time distribution and the worst case; then break a 40-bit toy RSA modulus from `cryptography/modular_arithmetic_dh_rsa`. Compare with [primal](https://github.com/huonw/primal) and [num-prime](https://github.com/cmpute/num-prime).

**Use in**: crypto groundwork, hash table sizing, contest problems.

**Pitfall**: `a * b % m` overflowing u64; the recipe's `mulmod` exists for this; write a failing test with m near 2^63.

---

## numerics_and_math

**Prove you got it**
1. How do you measure an observed convergence order from two step sizes, and what do you expect for bisection, secant and Newton?
2. Why does Kahan or pairwise summation matter, and for what input does naive summation lose all digits?

**Try next**
- [S] Add Romberg integration and confirm error order improves as theory predicts.
- [M] Add adaptive Simpson with an error tolerance guarantee; oracle: closed-form integrals of 5 functions, including one with a singular derivative.
- [A] Solve a 1D heat equation or a 2D Poisson problem with your `cg`, comparing iteration counts versus problem size; check against the analytic solution and read [ndarray](https://github.com/rust-ndarray/ndarray) / [nalgebra](https://github.com/dimforge/nalgebra) for solvers. Use `fft` to make a spectrogram of a recorded WAV and compare against [RustFFT](https://github.com/ejmahler/RustFFT).

**Use in**: signal processing, scientific code.

**Pitfall**: testing against one input where error cancels; use at least 5 inputs with different conditioning.

---

## physics_simulation

**Prove you got it**
1. Explain why explicit Euler gains energy on a harmonic oscillator while symplectic Euler keeps it bounded; verify with the recipe's `energy_run` for 10^5 steps.
2. Why a fixed timestep with accumulator (`run_fixed`) instead of a variable dt?

**Try next**
- [S] Plot (ASCII) energy drift for RK4 versus velocity Verlet over 10^6 steps and report the slope.
- [M] Add a double pendulum and show sensitivity: two initial conditions 1e-9 apart, measure time to diverge.
- [A] Build an N-body simulation (n = 1,000) with a Barnes-Hut quadtree using `spatial_search_structures::Quadtree` ideas; compare accuracy against direct O(n^2) forces and report the speed-up for several theta values. Read the broad design in [rapier](https://github.com/dimforge/rapier).

**Use in**: game physics, orbital demos.

**Pitfall**: letting a spring system with a large stiffness take a timestep above its stability limit; compute the limit and test just under and over it.

---

## prng_zoo

**Prove you got it**
1. Show that bit 0 of the LCG alternates, and bit k has period 2^(k+1).
2. Why is `x % n` biased for bounded integers, and how does Lemire's method avoid it? Produce numbers: for n = 3 on a tiny 8-bit generator, what are the exact residue counts?

**Try next**
- [S] Run `chi_square` on each generator's low byte and top byte.
- [M] Add xoshiro256** and test with a birthday-spacings test you write; show a generator that fails.
- [A] Implement a few tests of PractRand/TestU01 flavour (runs, serial correlation, gap test) and run them on 10^9 outputs of each generator; summarise. Read [rand](https://github.com/rust-random/rand) and [rand's generator crates](https://github.com/rust-random/rngs).

**Use in**: simulation, deterministic seeded tests (all cookbook oracles).

**Pitfall**: seeding two streams with consecutive seeds; use SplitMix64 to expand seeds.

---

## spatial_search_structures

**Prove you got it**
1. In the k-d tree `nn`, when can the far side be skipped? State the pruning inequality.
2. What is the curse of dimensionality in k-d trees, and which counted-work statistic shows it?

**Try next**
- [S] Compare nodes visited by k-d tree versus brute force for d = 2, 4, 8, 16, 32 on 10^5 uniform points.
- [M] Add delete-by-rebuild with a lazy deletion fraction; oracle: brute force over live points.
- [A] Build a BK-tree or VP-tree spell checker for a real word list (`/usr/share/dict/words`) and measure queries/second at edit distance 1 and 2. Compare k-d tree behaviour with [kiddo](https://github.com/sdd/kiddo) and R-tree with [rstar](https://github.com/georust/rstar).

**Use in**: nearest-neighbour queries, GIS.

**Pitfall**: duplicate points and ties in median splits creating unbalanced trees.

---

## decimal_fixed_point_money

**Prove you got it**
1. Why does the recipe verify `div_round` exhaustively for n in -60..=60 and divisors 1..12, and which rounding modes disagree on 2.5 and -2.5?
2. How does `allocate` split 100 cents three ways and which invariant must hold afterwards?

**Try next**
- [S] Add a currency with 0 decimals (JPY) and 3 decimals (KWD) and test `format_money`/`parse_money` round trips.
- [M] Compute compound interest over 360 months with Decimal and HalfEven versus `f64`; report the accumulated difference and the first month they diverge by 1 cent.
- [A] Build a double-entry ledger from a real bank CSV export: assert the sum of postings is zero per transaction and that totals equal the statement; add property tests with random split/allocate sequences.

**Use in**: billing, invoicing, payments. Real: [DataFusion](https://github.com/apache/datafusion) stores decimals as scaled integers (its Decimal128 type is a scaled i128).

**Pitfall**: rounding each line and then summing versus summing then rounding; the legal rule differs by jurisdiction.

---

## floating_point_ieee754

**Prove you got it**
1. Decode 0.1f32 by hand into sign, exponent, mantissa and verify with `to_bits`; what is its exact decimal value?
2. Why is comparing floats with `==` after summation wrong, and what does `ulps_between` give you that an absolute epsilon does not?

**Try next**
- [S] Show catastrophic cancellation: compute (1 - cos x) / x^2 for x = 1e-8 naively and with the half-angle identity.
- [M] Add Neumaier summation and compare error against Kahan, pairwise and naive on adversarial input (1e16, 1.0, -1e16 repeated).
- [A] Use the soft-float to implement fused multiply-add for the minifloat and check exhaustively against an exact reference; then build a total-order sort for f64 and compare with `f64::total_cmp` on 10^6 random bit patterns.

**Use in**: float test tolerances, reproducible analytics sums. Real: Rust's [libm](https://github.com/rust-lang/libm) shows how functions are built on exact bit manipulation.

**Pitfall**: assuming `f32` code ported to `f64` behaves the same; subnormals and rounding differ.

---

## Chapter projects

### Small (about 2 hours): "random number lab"
Combine `prng_zoo`, `data_structures/sampling`, `data_structures/hash_functions`.
- Milestones: (1) uniform floats, bounded ints, Gaussian; (2) chi-square and serial-correlation tests; (3) reservoir sampler check on a stream.
- Acceptance: a deliberately bad LCG fails at least two of your tests; the good ones pass at 99% across 100 seeds.
- Stretch: write a `Hasher` from an RNG and measure avalanche.

### Intermediate (about a weekend): "geometry toolbox with a picture"
Combine `computational_geometry`, `spatial_search_structures`, `graphics/lines_curves_fills`, `graphics/png_ppm_writer`.
- Milestones: (1) hull, triangulation and closest pair of 10^3 points; (2) draw them to PNG; (3) range and nearest queries with a k-d tree; (4) visual regression via checksum of the rendered image.
- Acceptance: every geometry routine matches its brute-force oracle on 10^3 random seeds including degenerate inputs.
- Stretch: Voronoi diagram from Delaunay; compare with [geo](https://github.com/georust/geo).

### Advanced (about a week): "toy physics engine and renderer"
Combine `physics_simulation`, `numerics_and_math` (linear solvers), `spatial_search_structures` (broad phase), `graphics/vec3_matrix_quaternion`, `graphics/software_rasterizer`, and `perf/bench_harness`.
- Milestones: (1) rigid circles with a grid broad phase; (2) impulse resolution with restitution; (3) energy and momentum assertions; (4) render frames to PNG; (5) 10^4 bodies at stable framerate.
- Acceptance: momentum conserved to 1e-9 in a closed system; broad phase returns the same pair set as brute force.
- Stretch: stacking stability, comparison with [rapier](https://github.com/dimforge/rapier).
