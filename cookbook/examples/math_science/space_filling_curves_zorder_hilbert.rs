//! RECIPE 844 (math-science): space-filling curves: Morton/Z-order, Hilbert, BIGMIN/LITMAX, range decomposition, data-skipping layouts.
//! LEVEL: intermediate
//! Use in: multi-dimensional clustering of tables (Delta/Iceberg Z-order), spatial indexes, geohash/S2 cells, cache-friendly array layouts.   Run: cargo run --release --example space_filling_curves_zorder_hilbert
//! COMMON USES: Delta Lake `OPTIMIZE ... ZORDER BY` and Iceberg sort/z-order rewrite (data skipping on several columns),
//! Databricks liquid clustering (Hilbert), UB-trees (Z-order keys in a B-tree, queried with BIGMIN), geohash (Morton on lon/lat
//! bits, Base32 text), Google S2 (Hilbert curve on the faces of a cube, 64-bit cell ids whose range contains all descendants),
//! texture/tile swizzling on GPUs, Morton-ordered octrees and N-body codes, cache-oblivious matrix layouts.
#![allow(dead_code)]
//!
//! WHAT. A space-filling curve maps a d-dimensional grid point to a 1-D index so that points close in space tend to be close in the
//! index; a B-tree or a sorted file layout over that index then answers box queries with a few index ranges.
//!   * MORTON (Z-order): interleave the bits of the coordinates. Cheap: with BMI2 it is one PDEP per coordinate; WITHOUT it the
//!     "magic number" trick spreads bits by log steps (shift, OR, mask): x -> x|x<<16 & 0x0000FFFF0000FFFF -> ... -> 0x5555...
//!     (2-D) and the 0x1249... masks (3-D, 21 bits per coordinate). Decoding is the same steps reversed. Z has long JUMPS where the
//!     curve crosses a quadrant boundary (the Z "diagonal").
//!   * HILBERT: the curve never jumps (consecutive indices are always adjacent cells) so it clusters better, but encoding is
//!     stateful: each quadrant rotates/reflects the sub-curve. Two implementations: Skilling's Gray-code "transpose" algorithm
//!     (works for any dimension; used for 2-D and 3-D) and the classic rotate-and-flip xy2d loop (2-D only, cross-checked).
//!   * RANGE QUERY on a Z-ordered sorted table: scan from zmin to zmax but skip the stretches outside the box: when the current key z
//!     is outside, BIGMIN(z) = the smallest in-box key greater than z (Tropf-Herzog), computed by one pass over the bits; LITMAX is its
//!     mirror (largest in-box key smaller than z), used to end a run. Both are tested against brute force.
//!   * INTERVAL SPLITTING (works for Z and Hilbert): an aligned 2^k-sided cell occupies ONE contiguous index range of 4^k (8^k in 3-D);
//!     recurse over the quad/octree, emit the range of every cell fully inside the box, drop disjoint cells, split partial ones, then
//!     merge adjacent ranges. `coarsen` merges the smallest gaps to cap the number of ranges (S2's region coverer trades the same way).
//!   * CLUSTERING METRICS: the CLUSTERING NUMBER of a query box is how many index runs cover it (= disk seeks); the JUMP DISTANCE is
//!     |index(p) - index(q)| over grid neighbours. Row-major wins on one axis and loses on the other; Hilbert is the most balanced.
//!   * DATA SKIPPING: sort a 200k-row table by (a), (a,b), Z(a,b) or Hilbert(a,b), cut it into files of 1000 rows, keep per-file
//!     min/max of both columns and prune files whose box misses the query: lexicographic order is perfect for `a` and useless for `b`,
//!     a space-filling curve is decent for both and best for conjunctions. That is Delta's Z-order and the reason it exists.
//!   * Hierarchical cell systems (described): geohash = Morton on (lon,lat) in base 32, prefix = containing cell; S2 = Hilbert per cube
//!     face with 30 levels, a cell id's range covers its children (the interval-splitting property above); H3 = hexagons, indexes
//!     are hierarchical but cells are not nested exactly, so region covering is approximate.
//!
//! CHECKS. Exhaustive encode/decode round trips and bijection for every grid up to 2^12 points (2-D, orders 1..6; 3-D, orders 1..4)
//! including the Hilbert adjacency invariant (consecutive indices differ by exactly 1 in exactly one coordinate); random round trips
//! at 32 bits (2-D) and 21 bits (3-D) against a naive bit loop; the two Hilbert implementations agree up to a symmetry; BIGMIN and
//! LITMAX equal brute force on every out-of-box key of random small boxes; the interval decomposition of random boxes in 2-D (Z and
//! Hilbert) and 3-D (Z and Hilbert) contains exactly the points in the box (oracle: filter every grid point); `coarsen` only
//! adds false positives; BIGMIN scans over 100k Z-sorted points equal the filter oracle; data-skipping file counts are computed
//! for four layouts and the expected ordering is asserted.
//! Read: Skilling, "Programming the Hilbert curve" AIP Conf. Proc. 707 (2004) (cited by title; URL not checked)
//!       Tropf, Herzog, "Multidimensional Range Search in Dynamically Balanced Trees" (1981) (cited by title)
//!       Moon, Jagadish, Faloutsos, Saltz, "Analysis of the Clustering Properties of the Hilbert Space-Filling Curve" (cited by title)
//!       Z-order curve https://en.wikipedia.org/wiki/Z-order_curve   Hilbert curve https://en.wikipedia.org/wiki/Hilbert_curve
//!       S2 geometry overview http://s2geometry.io/devguide/s2cell_hierarchy
//!
//! TRY NEXT:
//!   - [S] Replace `spread2` by a loop over bits and time both with `Instant`; predict the speed-up of the magic-number version before measuring.
//!   - [S] Predict the clustering numbers of a 1x64 and a 64x1 box for row-major, Z and Hilbert, then run `check_locality` and explain any surprise.
//!   - [M] Add a 4-D Morton and Hilbert (the code is dimension-generic: which function needs a new mask table, and which does not?).
//!   - [M] In `skipping_experiment` add a skewed (Zipf) column and show how equal-width cells waste files; then sort by Hilbert of the RANKS instead of raw values.
//!   - [M] Port: encode (latitude, longitude) as a 32-bit-per-axis Morton key, write it as a geohash-style Base32 string and verify the prefix-containment property.
//!   - [A] Project: combine with `database_cracking_zone_maps_imprints` (zone maps) and `bplus_tree`: build a UB-tree whose leaf pages carry zone maps and
//!     answer boxes with BIGMIN jumps; compare page reads against a plain B+tree over (a,b).

use std::collections::BTreeMap;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

// ---------------- Morton: magic-number bit spreading (no PDEP) ----------------
fn spread2(x: u32) -> u64 {
    let mut x = x as u64;
    x = (x | x << 16) & 0x0000_FFFF_0000_FFFF;
    x = (x | x << 8) & 0x00FF_00FF_00FF_00FF;
    x = (x | x << 4) & 0x0F0F_0F0F_0F0F_0F0F;
    x = (x | x << 2) & 0x3333_3333_3333_3333;
    x = (x | x << 1) & 0x5555_5555_5555_5555;
    x
}
fn compact2(mut x: u64) -> u32 {
    x &= 0x5555_5555_5555_5555;
    x = (x | x >> 1) & 0x3333_3333_3333_3333;
    x = (x | x >> 2) & 0x0F0F_0F0F_0F0F_0F0F;
    x = (x | x >> 4) & 0x00FF_00FF_00FF_00FF;
    x = (x | x >> 8) & 0x0000_FFFF_0000_FFFF;
    x = (x | x >> 16) & 0x0000_0000_FFFF_FFFF;
    x as u32
}
fn spread3(x: u32) -> u64 {
    let mut x = x as u64 & 0x1f_ffff; // 21 bits
    x = (x | x << 32) & 0x001f_0000_0000_ffff;
    x = (x | x << 16) & 0x001f_0000_ff00_00ff;
    x = (x | x << 8) & 0x100f_00f0_0f00_f00f;
    x = (x | x << 4) & 0x10c3_0c30_c30c_30c3;
    x = (x | x << 2) & 0x1249_2492_4924_9249;
    x
}
fn compact3(mut x: u64) -> u32 {
    x &= 0x1249_2492_4924_9249;
    x = (x ^ (x >> 2)) & 0x10c3_0c30_c30c_30c3;
    x = (x ^ (x >> 4)) & 0x100f_00f0_0f00_f00f;
    x = (x ^ (x >> 8)) & 0x001f_0000_ff00_00ff;
    x = (x ^ (x >> 16)) & 0x001f_0000_0000_ffff;
    x = (x ^ (x >> 32)) & 0x1f_ffff;
    x as u32
}
/// Reference: interleave bit by bit; dimension i of each group sits at bit position i.
fn naive_interleave(p: &[u32], b: u32) -> u64 {
    let mut z = 0u64;
    for bit in 0..b {
        for (i, &c) in p.iter().enumerate() {
            z |= ((c >> bit) as u64 & 1) << (bit as usize * p.len() + i);
        }
    }
    z
}

// ---------------- Hilbert: Skilling's Gray-code transpose algorithm (any dimension) ----------------
fn axes_to_transpose(x: &mut [u64], b: u32) {
    let n = x.len();
    let m = 1u64 << (b - 1);
    let mut q = m;
    while q > 1 {
        let p = q - 1;
        for i in 0..n {
            if x[i] & q != 0 {
                x[0] ^= p; // invert
            } else {
                let t = (x[0] ^ x[i]) & p; // exchange low bits
                x[0] ^= t;
                x[i] ^= t;
            }
        }
        q >>= 1;
    }
    for i in 1..n {
        x[i] ^= x[i - 1]; // Gray encode
    }
    let mut t = 0;
    let mut q = m;
    while q > 1 {
        if x[n - 1] & q != 0 {
            t ^= q - 1;
        }
        q >>= 1;
    }
    for v in x.iter_mut() {
        *v ^= t;
    }
}
fn transpose_to_axes(x: &mut [u64], b: u32) {
    let n = x.len();
    let big_n = 2u64 << (b - 1);
    let t = x[n - 1] >> 1; // Gray decode
    for i in (1..n).rev() {
        x[i] ^= x[i - 1];
    }
    x[0] ^= t;
    let mut q = 2;
    while q != big_n {
        let p = q - 1;
        for i in (0..n).rev() {
            if x[i] & q != 0 {
                x[0] ^= p;
            } else {
                let t = (x[0] ^ x[i]) & p;
                x[0] ^= t;
                x[i] ^= t;
            }
        }
        q <<= 1;
    }
}
fn hilbert_encode(p: &[u32], b: u32) -> u64 {
    let mut x: Vec<u64> = p.iter().map(|&v| v as u64).collect();
    axes_to_transpose(&mut x, b);
    let mut idx = 0u64;
    for bit in (0..b).rev() {
        for v in &x {
            idx = idx << 1 | (v >> bit & 1); // transposed form -> one integer, coordinate 0 most significant per level
        }
    }
    idx
}
fn hilbert_decode(idx: u64, d: usize, b: u32) -> Vec<u32> {
    let mut x = vec![0u64; d];
    for bit in (0..b).rev() {
        for (i, v) in x.iter_mut().enumerate() {
            let pos = bit as usize * d + (d - 1 - i);
            *v |= (idx >> pos & 1) << bit;
        }
    }
    transpose_to_axes(&mut x, b);
    x.into_iter().map(|v| v as u32).collect()
}
/// Classic 2-D rotate-and-flip version (Wikipedia xy2d) as an independent implementation.
fn hilbert_xy2d(b: u32, mut x: u64, mut y: u64) -> u64 {
    let n = 1u64 << b;
    let mut d = 0;
    let mut s = n / 2;
    while s > 0 {
        let rx = (x & s > 0) as u64;
        let ry = (y & s > 0) as u64;
        d += s * s * ((3 * rx) ^ ry);
        if ry == 0 {
            if rx == 1 {
                x = n - 1 - x;
                y = n - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        s /= 2;
    }
    d
}

// ---------------- a curve abstraction ----------------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    RowMajor,
    Morton,
    Hilbert,
}
#[derive(Clone, Copy, Debug)]
struct Curve {
    kind: Kind,
    d: usize,
    b: u32, // bits per coordinate
}
impl Curve {
    fn encode(&self, p: &[u32]) -> u64 {
        debug_assert_eq!(p.len(), self.d);
        match (self.kind, self.d) {
            (Kind::RowMajor, _) => p.iter().fold(0u64, |a, &c| a << self.b | c as u64),
            (Kind::Morton, 2) => spread2(p[0]) | spread2(p[1]) << 1,
            (Kind::Morton, 3) => spread3(p[0]) | spread3(p[1]) << 1 | spread3(p[2]) << 2,
            (Kind::Morton, _) => naive_interleave(p, self.b),
            (Kind::Hilbert, _) => hilbert_encode(p, self.b),
        }
    }
    fn decode(&self, i: u64) -> Vec<u32> {
        match (self.kind, self.d) {
            (Kind::RowMajor, _) => (0..self.d)
                .rev()
                .map(|k| (i >> (k as u32 * self.b) & ((1u64 << self.b) - 1)) as u32)
                .collect(),
            (Kind::Morton, 2) => vec![compact2(i), compact2(i >> 1)],
            (Kind::Morton, 3) => vec![compact3(i), compact3(i >> 1), compact3(i >> 2)],
            (Kind::Morton, _) => (0..self.d)
                .map(|k| {
                    (0..self.b).fold(0u32, |a, bit| {
                        a | ((i >> (bit as usize * self.d + k) & 1) as u32) << bit
                    })
                })
                .collect(),
            (Kind::Hilbert, _) => hilbert_decode(i, self.d, self.b),
        }
    }
    fn total_bits(&self) -> u32 {
        self.d as u32 * self.b
    }
    fn cells(&self) -> u64 {
        1u64 << self.total_bits()
    }
}

// ---------------- BIGMIN / LITMAX on Morton keys ----------------
/// Mask of the bit positions <= `bit` that belong to the same dimension as `bit` (positions p with p % d == bit % d).
fn dim_mask(d: usize, bit: u32) -> u64 {
    let mut m = 0;
    let mut p = bit as i64;
    while p >= 0 {
        m |= 1u64 << p;
        p -= d as i64;
    }
    m
}
fn load_1000(z: u64, bit: u32, d: usize) -> u64 {
    (z & !dim_mask(d, bit)) | 1 << bit // this coordinate: 1 followed by zeros
}
fn load_0111(z: u64, bit: u32, d: usize) -> u64 {
    (z & !dim_mask(d, bit)) | (dim_mask(d, bit) & !(1u64 << bit)) // this coordinate: 0 followed by ones
}
/// Smallest key inside the box [zmin,zmax] that is > z. Precondition: zmin <= z < zmax and z not in the box.
fn bigmin(z: u64, mut zmin: u64, mut zmax: u64, d: usize, nb: u32) -> u64 {
    let mut big = u64::MAX;
    for bit in (0..nb).rev() {
        let (v, mn, mx) = (z >> bit & 1, zmin >> bit & 1, zmax >> bit & 1);
        match (v, mn, mx) {
            (0, 0, 0) | (1, 1, 1) => {}
            (0, 0, 1) => {
                big = load_1000(zmin, bit, d); // the upper half-box starts here: remember it
                zmax = load_0111(zmax, bit, d); // and keep searching the lower half
            }
            (0, 1, 1) => return zmin,
            (1, 0, 0) => return big,
            (1, 0, 1) => zmin = load_1000(zmin, bit, d), // z is in the upper half: drop the lower half
            _ => unreachable!("zmin <= zmax violated in some coordinate"),
        }
    }
    big
}
/// Largest key inside the box that is < z.
fn litmax(z: u64, mut zmin: u64, mut zmax: u64, d: usize, nb: u32) -> u64 {
    let mut lit = 0;
    for bit in (0..nb).rev() {
        let (v, mn, mx) = (z >> bit & 1, zmin >> bit & 1, zmax >> bit & 1);
        match (v, mn, mx) {
            (0, 0, 0) | (1, 1, 1) => {}
            (0, 0, 1) => zmax = load_0111(zmax, bit, d),
            (0, 1, 1) => return lit,
            (1, 0, 0) => return zmax,
            (1, 0, 1) => {
                lit = load_0111(zmax, bit, d);
                zmin = load_1000(zmin, bit, d);
            }
            _ => unreachable!("zmin <= zmax violated in some coordinate"),
        }
    }
    lit
}
fn in_box(c: &Curve, key: u64, lo: &[u32], hi: &[u32]) -> bool {
    c.decode(key)
        .iter()
        .zip(lo.iter().zip(hi))
        .all(|(&v, (&l, &h))| v >= l && v <= h)
}

// ---------------- interval splitting ----------------
/// Index ranges (inclusive, sorted, merged) covering exactly the points of the inclusive box [lo,hi]. Z and Hilbert only.
fn decompose(c: &Curve, lo: &[u32], hi: &[u32], cells_visited: &mut u64) -> Vec<(u64, u64)> {
    fn rec(
        c: &Curve,
        corner: &mut Vec<u32>,
        level: u32,
        lo: &[u32],
        hi: &[u32],
        out: &mut Vec<(u64, u64)>,
        visited: &mut u64,
    ) {
        *visited += 1;
        let side = 1u64 << level;
        let (mut inside, mut outside) = (true, false);
        for i in 0..c.d {
            let (cl, ch) = (corner[i] as u64, corner[i] as u64 + side - 1);
            if ch < lo[i] as u64 || cl > hi[i] as u64 {
                outside = true;
            }
            if cl < lo[i] as u64 || ch > hi[i] as u64 {
                inside = false;
            }
        }
        if outside {
            return;
        }
        if inside {
            let span = 1u64 << (c.d as u32 * level);
            let start = c.encode(corner) & !(span - 1); // an aligned cell is one contiguous, aligned index block
            out.push((start, start + span - 1));
            return;
        }
        for child in 0..1u32 << c.d {
            let saved = corner.clone();
            for i in 0..c.d {
                if child >> i & 1 == 1 {
                    corner[i] += 1 << (level - 1);
                }
            }
            rec(c, corner, level - 1, lo, hi, out, visited);
            *corner = saved;
        }
    }
    let mut out = vec![];
    rec(c, &mut vec![0; c.d], c.b, lo, hi, &mut out, cells_visited);
    out.sort_unstable();
    let mut merged: Vec<(u64, u64)> = vec![];
    for (a, b) in out {
        match merged.last_mut() {
            Some(l) if l.1 + 1 == a => l.1 = b,
            _ => merged.push((a, b)),
        }
    }
    merged
}
/// Reduce to at most `k` ranges by repeatedly closing the smallest gap (adds false positives, never drops a true point).
fn coarsen(mut iv: Vec<(u64, u64)>, k: usize) -> Vec<(u64, u64)> {
    while iv.len() > k.max(1) {
        let g = (0..iv.len() - 1)
            .min_by_key(|&i| iv[i + 1].0 - iv[i].1)
            .unwrap();
        iv[g].1 = iv[g + 1].1;
        iv.remove(g + 1);
    }
    iv
}
fn covered(iv: &[(u64, u64)], key: u64) -> bool {
    let i = iv.partition_point(|r| r.1 < key);
    i < iv.len() && iv[i].0 <= key
}

// ---------------- checks ----------------
fn check_morton_and_hilbert_exhaustive() {
    let mut cases = 0;
    for (d, max_b) in [(2usize, 6u32), (3, 4)] {
        for b in 1..=max_b {
            let (z, h) = (
                Curve {
                    kind: Kind::Morton,
                    d,
                    b,
                },
                Curve {
                    kind: Kind::Hilbert,
                    d,
                    b,
                },
            );
            let mut seen_z = vec![false; z.cells() as usize];
            let mut seen_h = vec![false; h.cells() as usize];
            let mut prev: Option<Vec<u32>> = None;
            for i in 0..z.cells() {
                let pz = z.decode(i);
                assert!(pz.iter().all(|&v| (v as u64) < 1 << b));
                assert_eq!(z.encode(&pz), i, "Morton round trip d={d} b={b} i={i}");
                assert_eq!(
                    z.encode(&pz),
                    naive_interleave(&pz, b),
                    "magic numbers differ from the bit loop"
                );
                seen_z[i as usize] = true;
                let ph = h.decode(i);
                assert!(
                    ph.iter().all(|&v| (v as u64) < 1 << b),
                    "Hilbert decode out of range"
                );
                assert_eq!(h.encode(&ph), i, "Hilbert round trip d={d} b={b} i={i}");
                seen_h[h.encode(&ph) as usize] = true;
                if let Some(p) = &prev {
                    let dist: u32 = p.iter().zip(&ph).map(|(&a, &c)| a.abs_diff(c)).sum();
                    assert_eq!(
                        dist, 1,
                        "Hilbert neighbours i-1,i must be adjacent cells (d={d} b={b} i={i})"
                    );
                }
                prev = Some(ph);
                cases += 1;
            }
            assert!(
                seen_z.iter().all(|&s| s) && seen_h.iter().all(|&s| s),
                "bijection"
            );
        }
    }
    // the Wikipedia xy2d agrees with Skilling up to one of the 16 symmetries of the square (swap, flips, reversal)
    for b in 1..=6u32 {
        let n = 1u64 << b;
        let h = Curve {
            kind: Kind::Hilbert,
            d: 2,
            b,
        };
        let maps: Vec<Box<dyn Fn(u64, u64) -> (u64, u64)>> = vec![
            Box::new(|x, y| (x, y)),
            Box::new(|x, y| (y, x)),
            Box::new(move |x, y| (n - 1 - x, y)),
            Box::new(move |x, y| (x, n - 1 - y)),
            Box::new(move |x, y| (n - 1 - y, x)),
            Box::new(move |x, y| (y, n - 1 - x)),
            Box::new(move |x, y| (n - 1 - x, n - 1 - y)),
            Box::new(move |x, y| (n - 1 - y, n - 1 - x)),
        ];
        let found = maps.iter().enumerate().any(|(_, m)| {
            [false, true].iter().any(|&rev| {
                (0..n).all(|x| {
                    (0..n).all(|y| {
                        let (a, c) = m(x, y);
                        let w = hilbert_xy2d(b, a, c);
                        h.encode(&[x as u32, y as u32]) == if rev { n * n - 1 - w } else { w }
                    })
                })
            })
        });
        assert!(
            found,
            "two Hilbert implementations must describe the same curve up to symmetry (b={b})"
        );
    }
    println!(
        "exhaustive: {cases} (curve,grid,index) cases: Morton/Hilbert round trips, bijection, Hilbert adjacency; xy2d == Skilling up to symmetry"
    );
}

fn check_random_wide_coordinates() {
    let mut r = Rng(0x2D17);
    for _ in 0..50_000 {
        let (x, y) = (r.next() as u32, r.next() as u32);
        let z2 = Curve {
            kind: Kind::Morton,
            d: 2,
            b: 32,
        };
        let k = z2.encode(&[x, y]);
        assert_eq!(z2.decode(k), vec![x, y]);
        assert_eq!(k, naive_interleave(&[x, y], 32));
        let h2 = Curve {
            kind: Kind::Hilbert,
            d: 2,
            b: 32,
        };
        assert_eq!(h2.decode(h2.encode(&[x, y])), vec![x, y]);
        let p3: Vec<u32> = (0..3).map(|_| r.next() as u32 & 0x1f_ffff).collect();
        let z3 = Curve {
            kind: Kind::Morton,
            d: 3,
            b: 21,
        };
        assert_eq!(z3.decode(z3.encode(&p3)), p3);
        assert_eq!(z3.encode(&p3), naive_interleave(&p3, 21));
        let h3 = Curve {
            kind: Kind::Hilbert,
            d: 3,
            b: 21,
        };
        assert_eq!(h3.decode(h3.encode(&p3)), p3);
    }
    println!(
        "random round trips: 2-D at 32 bits and 3-D at 21 bits per coordinate, Morton (magic numbers vs bit loop) and Hilbert"
    );
}

fn random_box(r: &mut Rng, d: usize, b: u32, max_side: u64) -> (Vec<u32>, Vec<u32>) {
    let n = 1u64 << b;
    let mut lo = vec![];
    let mut hi = vec![];
    for _ in 0..d {
        let side = 1 + r.below(max_side.min(n));
        let l = r.below(n - side + 1);
        lo.push(l as u32);
        hi.push((l + side - 1) as u32);
    }
    (lo, hi)
}
fn check_bigmin_litmax() {
    let mut r = Rng(31337);
    let mut tested = 0;
    for (d, b) in [(2usize, 4u32), (2, 5), (3, 3)] {
        let c = Curve {
            kind: Kind::Morton,
            d,
            b,
        };
        for _ in 0..60 {
            let (lo, hi) = random_box(&mut r, d, b, 1 << b);
            let (zmin, zmax) = (c.encode(&lo), c.encode(&hi));
            let inside: Vec<u64> = (zmin..=zmax).filter(|&k| in_box(&c, k, &lo, &hi)).collect();
            for z in zmin..=zmax {
                if in_box(&c, z, &lo, &hi) {
                    continue;
                }
                let want_big = inside.iter().copied().find(|&k| k > z).unwrap();
                let want_lit = inside.iter().copied().rev().find(|&k| k < z).unwrap();
                assert_eq!(
                    bigmin(z, zmin, zmax, d, c.total_bits()),
                    want_big,
                    "BIGMIN d={d} b={b} box {lo:?}-{hi:?} z={z}"
                );
                assert_eq!(
                    litmax(z, zmin, zmax, d, c.total_bits()),
                    want_lit,
                    "LITMAX d={d} b={b} box {lo:?}-{hi:?} z={z}"
                );
                tested += 1;
            }
        }
    }
    println!("BIGMIN/LITMAX equal brute force on {tested} out-of-box keys (2-D and 3-D)");
}

fn check_decomposition() {
    let mut r = Rng(0xDEC0);
    let mut stats: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new(); // name -> (boxes, ranges, cells visited)
    for (d, b, boxes) in [(2usize, 8u32, 120), (3, 5, 60)] {
        for kind in [Kind::Morton, Kind::Hilbert] {
            let c = Curve { kind, d, b };
            for _ in 0..boxes {
                let (lo, hi) = random_box(&mut r, d, b, 1 << (b - 1));
                let mut visited = 0;
                let iv = decompose(&c, &lo, &hi, &mut visited);
                for key in 0..c.cells() {
                    assert_eq!(
                        covered(&iv, key),
                        in_box(&c, key, &lo, &hi),
                        "{kind:?} d={d}: key {key} wrongly (un)covered for box {lo:?}-{hi:?}"
                    );
                }
                let vol: u64 = lo
                    .iter()
                    .zip(&hi)
                    .map(|(&l, &h)| (h - l + 1) as u64)
                    .product();
                assert_eq!(
                    iv.iter().map(|r| r.1 - r.0 + 1).sum::<u64>(),
                    vol,
                    "range lengths must add up to the box volume"
                );
                // coarsening only adds false positives
                for k in [1usize, 3, 8] {
                    let co = coarsen(iv.clone(), k);
                    assert!(co.len() <= k.max(1).max(if iv.len() < k { iv.len() } else { 0 }));
                    assert!(
                        (0..c.cells())
                            .filter(|&key| in_box(&c, key, &lo, &hi))
                            .all(|key| covered(&co, key)),
                        "coarsen dropped a point"
                    );
                }
                let e = stats.entry(format!("{kind:?} {d}-D")).or_default();
                *e = (e.0 + 1, e.1 + iv.len() as u64, e.2 + visited);
            }
        }
    }
    println!(
        "interval decomposition == filter oracle (every grid point checked per box); ranges per box and cells visited:"
    );
    for (name, (n, ranges, visited)) in &stats {
        println!(
            "  {name:>12}: {:.1} ranges/box, {:.0} cells visited/box",
            *ranges as f64 / *n as f64,
            *visited as f64 / *n as f64
        );
    }
    let (zm, hi) = (&stats["Morton 2-D"], &stats["Hilbert 2-D"]);
    assert!(
        hi.1 <= zm.1,
        "Hilbert should need no more ranges than Z on the same boxes"
    );
}

fn check_bigmin_scan() {
    let c = Curve {
        kind: Kind::Morton,
        d: 2,
        b: 16,
    };
    let mut r = Rng(77);
    let mut pts: Vec<(u64, (u32, u32))> = (0..100_000)
        .map(|_| {
            let (x, y) = (r.below(1 << 16) as u32, r.below(1 << 16) as u32);
            (c.encode(&[x, y]), (x, y))
        })
        .collect();
    pts.sort_unstable();
    let keys: Vec<u64> = pts.iter().map(|p| p.0).collect();
    let (mut visited_total, mut full_total, mut jumps_total) = (0usize, 0usize, 0usize);
    for _ in 0..200 {
        let (lo, hi) = random_box(&mut r, 2, 16, 8000);
        let want = pts
            .iter()
            .filter(|(_, (x, y))| *x >= lo[0] && *x <= hi[0] && *y >= lo[1] && *y <= hi[1])
            .count();
        let (zmin, zmax) = (c.encode(&lo), c.encode(&hi));
        let mut i = keys.partition_point(|&k| k < zmin);
        let (mut got, mut visited, mut jumps) = (0, 0, 0);
        while i < keys.len() && keys[i] <= zmax {
            visited += 1;
            if in_box(&c, keys[i], &lo, &hi) {
                got += 1;
                i += 1;
            } else {
                let nz = bigmin(keys[i], zmin, zmax, 2, 32);
                i += keys[i..].partition_point(|&k| k < nz); // seek past the dead stretch
                jumps += 1;
            }
        }
        assert_eq!(
            got, want,
            "BIGMIN scan lost or invented points in box {lo:?}-{hi:?}"
        );
        visited_total += visited;
        jumps_total += jumps;
        full_total +=
            keys[keys.partition_point(|&k| k < zmin)..keys.partition_point(|&k| k <= zmax)].len();
    }
    println!(
        "BIGMIN scan of 100k Z-sorted points, 200 boxes: visited {visited_total} keys (+{jumps_total} seeks) vs {full_total} for a naive zmin..zmax scan"
    );
    assert!(
        visited_total < full_total,
        "BIGMIN must skip dead stretches"
    );
}

fn clusters(c: &Curve, lo: &[u32], hi: &[u32]) -> usize {
    let mut keys = vec![];
    let mut p = lo.to_vec();
    loop {
        keys.push(c.encode(&p));
        let mut k = 0;
        loop {
            if k == c.d {
                keys.sort_unstable();
                return 1 + keys.windows(2).filter(|w| w[1] != w[0] + 1).count();
            }
            if p[k] < hi[k] {
                p[k] += 1;
                break;
            }
            p[k] = lo[k];
            k += 1;
        }
    }
}
fn check_locality() {
    let mut r = Rng(5150);
    let b = 8;
    let kinds = [Kind::RowMajor, Kind::Morton, Kind::Hilbert];
    println!(
        "\nclustering number (index runs needed to read a box; lower = fewer seeks), mean over 300 random boxes on a 256x256 grid:"
    );
    println!(
        "{:>10} {:>10} {:>10} {:>10}",
        "box w x h", "row-major", "Morton", "Hilbert"
    );
    let mut square_means = vec![];
    let mut thin: Vec<Vec<f64>> = vec![];
    for (w, h) in [(8u32, 8u32), (32, 32), (2, 64), (64, 2)] {
        let boxes: Vec<(Vec<u32>, Vec<u32>)> = (0..300)
            .map(|_| {
                let (x, y) = (
                    r.below(256 - w as u64) as u32,
                    r.below(256 - h as u64) as u32,
                );
                (vec![x, y], vec![x + w - 1, y + h - 1])
            })
            .collect();
        let means: Vec<f64> = kinds
            .iter()
            .map(|&kind| {
                let c = Curve { kind, d: 2, b };
                boxes
                    .iter()
                    .map(|(lo, hi)| clusters(&c, lo, hi))
                    .sum::<usize>() as f64
                    / boxes.len() as f64
            })
            .collect();
        println!(
            "{:>10} {:>10.1} {:>10.1} {:>10.1}",
            format!("{w}x{h}"),
            means[0],
            means[1],
            means[2]
        );
        if w == h {
            square_means.push(means);
        } else {
            thin.push(means);
        }
    }
    // row-major is wildly asymmetric (a 2x64 box is 2 runs, a 64x2 box is 64); Hilbert treats both orientations alike
    assert!(
        thin[0][0] * 10.0 < thin[1][0],
        "row-major: tall thin box ~free, wide thin box expensive: {thin:?}"
    );
    assert!(
        (thin[0][2] / thin[1][2]).max(thin[1][2] / thin[0][2]) < 1.3,
        "Hilbert should be nearly orientation-symmetric: {thin:?}"
    );
    assert!(
        thin[0][1] / thin[1][1] > 1.3 || thin[1][1] / thin[0][1] > 1.3,
        "Z is visibly asymmetric: x bits sit below y bits: {thin:?}"
    );
    for m in &square_means {
        assert!(
            m[2] <= m[1],
            "Hilbert should cluster squares at least as well as Z ({m:?})"
        );
    }
    // jump distance between grid neighbours
    println!(
        "jump distance |index(p) - index(neighbour)| over all 4-neighbours of a 128x128 grid:"
    );
    println!(
        "{:>10} {:>14} {:>12} {:>16}",
        "curve", "mean", "max", "% within 16"
    );
    let mut near_pct = vec![];
    for kind in kinds {
        let c = Curve { kind, d: 2, b: 7 };
        let (mut sum, mut max, mut near, mut n) = (0u128, 0u64, 0u64, 0u64);
        for x in 0..128u32 {
            for y in 0..128u32 {
                for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                    if nx < 128 && ny < 128 {
                        let dj = c.encode(&[x, y]).abs_diff(c.encode(&[nx, ny]));
                        sum += dj as u128;
                        max = max.max(dj);
                        near += (dj <= 16) as u64;
                        n += 1;
                    }
                }
            }
        }
        let mean = sum as f64 / n as f64;
        println!(
            "{:>10} {:>14.1} {:>12} {:>15.1}%",
            format!("{kind:?}"),
            mean,
            max,
            near as f64 / n as f64 * 100.0
        );
        near_pct.push(near as f64 / n as f64);
    }
    println!(
        "  (the MEAN is dominated by a few huge jumps at quadrant seams, so compare the fraction of neighbours that stay close)"
    );
    assert!(
        near_pct[0] < near_pct[1] && near_pct[1] <= near_pct[2],
        "neighbour locality should order row-major < Z <= Hilbert: {near_pct:?}"
    );
}

fn skipping_experiment() {
    const ROWS: usize = 200_000;
    const FILE: usize = 1000;
    const B: u32 = 16;
    let mut r = Rng(0x5C1B);
    let rows: Vec<(u32, u32)> = (0..ROWS)
        .map(|_| (r.below(1 << B) as u32, r.below(1 << B) as u32))
        .collect();
    let z = Curve {
        kind: Kind::Morton,
        d: 2,
        b: B,
    };
    let h = Curve {
        kind: Kind::Hilbert,
        d: 2,
        b: B,
    };
    let layouts: Vec<(&str, Vec<(u32, u32)>)> = {
        let sorted_by = |f: &dyn Fn(&(u32, u32)) -> u64| {
            let mut v = rows.clone();
            v.sort_by_key(|p| f(p));
            v
        };
        vec![
            ("arrival order", rows.clone()),
            (
                "sort by (a,b)",
                sorted_by(&|p| (p.0 as u64) << 32 | p.1 as u64),
            ),
            ("Z-order(a,b)", sorted_by(&|p| z.encode(&[p.0, p.1]))),
            ("Hilbert(a,b)", sorted_by(&|p| h.encode(&[p.0, p.1]))),
        ]
    };
    // three query shapes, 300 each: a-only strip (5% wide), b-only strip, and a 10% x 10% box
    let side = (1u64 << B) / 10;
    let strip = (1u64 << B) / 20;
    let mut qs: Vec<[u32; 4]> = vec![]; // lo_a, hi_a, lo_b, hi_b (inclusive)
    let kinds = ["a only", "b only", "a and b"];
    for kind in 0..3 {
        for _ in 0..300 {
            let (wa, wb) = match kind {
                0 => (strip, 1 << B),
                1 => (1 << B, strip),
                _ => (side, side),
            };
            let (la, lb) = (r.below((1 << B) - wa + 1), r.below((1 << B) - wb + 1));
            qs.push([
                la as u32,
                (la + wa - 1) as u32,
                lb as u32,
                (lb + wb - 1) as u32,
            ]);
        }
    }
    let truth: Vec<usize> = qs
        .iter()
        .map(|q| {
            rows.iter()
                .filter(|p| p.0 >= q[0] && p.0 <= q[1] && p.1 >= q[2] && p.1 <= q[3])
                .count()
        })
        .collect();
    println!(
        "\ndata skipping: {ROWS} rows in files of {FILE}; mean % of files that must be read (per-file min/max on both columns):"
    );
    println!(
        "{:>15} {:>9} {:>9} {:>9}",
        "layout", kinds[0], kinds[1], kinds[2]
    );
    let mut table = BTreeMap::new();
    for (name, data) in &layouts {
        let stats: Vec<[u32; 4]> = data
            .chunks(FILE)
            .map(|f| {
                [
                    f.iter().map(|p| p.0).min().unwrap(),
                    f.iter().map(|p| p.0).max().unwrap(),
                    f.iter().map(|p| p.1).min().unwrap(),
                    f.iter().map(|p| p.1).max().unwrap(),
                ]
            })
            .collect();
        let mut frac = [0f64; 3];
        for (qi, q) in qs.iter().enumerate() {
            let mut got = 0;
            let mut read = 0;
            for (fi, s) in stats.iter().enumerate() {
                if s[1] < q[0] || s[0] > q[1] || s[3] < q[2] || s[2] > q[3] {
                    continue; // pruned by min/max
                }
                read += 1;
                got += data[fi * FILE..((fi + 1) * FILE).min(data.len())]
                    .iter()
                    .filter(|p| p.0 >= q[0] && p.0 <= q[1] && p.1 >= q[2] && p.1 <= q[3])
                    .count();
            }
            assert_eq!(got, truth[qi], "{name}: pruning lost rows");
            frac[qi / 300] += read as f64 / stats.len() as f64 * 100.0 / 300.0;
        }
        println!(
            "{name:>15} {:>8.1}% {:>8.1}% {:>8.1}%",
            frac[0], frac[1], frac[2]
        );
        table.insert(*name, frac);
    }
    assert!(
        table["arrival order"].iter().all(|&f| f > 90.0),
        "unclustered data cannot be skipped"
    );
    assert!(
        table["sort by (a,b)"][0] < 10.0 && table["sort by (a,b)"][1] > 90.0,
        "lexicographic: great for a, useless for b"
    );
    for curve in ["Z-order(a,b)", "Hilbert(a,b)"] {
        let t = table[curve];
        assert!(
            t[1] < 45.0 && t[0] < 45.0,
            "{curve}: both single-column filters should skip a majority of files, got {t:?}"
        );
        assert!(
            t[2] < table["sort by (a,b)"][2],
            "{curve}: conjunctions should beat lexicographic order"
        );
    }
}

fn main() {
    check_morton_and_hilbert_exhaustive();
    check_random_wide_coordinates();
    check_bigmin_litmax();
    check_decomposition();
    check_bigmin_scan();
    check_locality();
    skipping_experiment();
    println!("all checks passed");
}
