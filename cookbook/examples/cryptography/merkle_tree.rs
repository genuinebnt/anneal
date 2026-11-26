//! RECIPE 154 (integrity): SHA-256 from scratch + Merkle tree, inclusion proofs, append-only log, anti-entropy.
//! LEVEL: advanced
//! Use in: replica repair, tamper-evident logs, content-addressed storage.   Run: cargo run --release --example merkle_tree
#![allow(dead_code)]
//!
//! WHAT: a Merkle tree hashes leaves, then hashes pairs of hashes up to one root. Equal roots mean equal data
//! (up to hash collisions); a different root means "something differs", and the tree tells you WHERE.
//!
//! HOW (design choices, following RFC 6962 "Certificate Transparency"):
//!  - Domain separation: leaf = H(0x00 || data), node = H(0x01 || left || right). Without it, the 64 bytes
//!    `h(a)||h(b)` could be presented as a *leaf* and hash to the same value as the inner node (second-preimage
//!    attack on the tree). `naive_forgery` below demonstrates this on a prefix-free tree.
//!  - Odd node count: NO duplication of the last node (Bitcoin-style duplication makes [a,b,c] and [a,b,c,c]
//!    share a root). Instead split n leaves at the largest power of two k < n: MTH(D) = H(1 || MTH(D[..k]) || MTH(D[k..])).
//!    A lone leaf is simply promoted into a shallower position. This shape is also exactly what an append-only
//!    "frontier" of perfect subtrees produces, so the incremental log and the batch tree agree (tested).
//!  - Proof = sibling hashes bottom-up, each tagged with which side it sits on; verify re-folds to the root.
//!  - Anti-entropy: two replicas with the same leaf slots compare roots, then descend only into children whose
//!    hashes differ: cost ~ O(k log n) hash comparisons for k differing leaves, instead of n.
//!
//! COMMON USES: Git and Mercurial-style content-addressed object graphs (hash-linked trees), Bitcoin block
//! headers committing to transactions (SPV proofs), Certificate Transparency logs (RFC 6962), Dynamo-style
//! anti-entropy and Cassandra's repair, which compare Merkle trees of key ranges to find divergent replicas,
//! and content-addressed stores/packages that verify chunks against a trusted root.
//! Read: https://www.rfc-editor.org/rfc/rfc6962 (section 2.1)
//! Read: https://csrc.nist.gov/pubs/fips/180-4/upd1/final (SHA-256 spec)
//! Read: https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf
use std::sync::OnceLock;

// ---------------------------------------------------------------- SHA-256 (FIPS 180-4)

type Hash = [u8; 32];

fn primes(count: usize) -> Vec<u128> {
    let mut v: Vec<u128> = Vec::new();
    let mut c = 2u128;
    while v.len() < count {
        if v.iter().all(|p| c % p != 0) {
            v.push(c);
        }
        c += 1;
    }
    v
}

/// floor(cbrt(x)) by binary search; x < 2^108 so cubes fit u128.
fn icbrt(x: u128) -> u128 {
    let (mut lo, mut hi) = (0u128, 1u128 << 36);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if mid * mid * mid <= x {
            lo = mid
        } else {
            hi = mid - 1
        }
    }
    lo
}

/// The spec defines the constants as fractional bits of sqrt/cbrt of primes. Deriving them (instead of
/// pasting 72 magic numbers) removes a typo risk; the official test vectors below prove they are right.
fn constants() -> &'static ([u32; 8], [u32; 64]) {
    static C: OnceLock<([u32; 8], [u32; 64])> = OnceLock::new();
    C.get_or_init(|| {
        let ps = primes(64);
        let mut h0 = [0u32; 8];
        for (i, p) in ps.iter().take(8).enumerate() {
            h0[i] = (p << 64).isqrt() as u32; // low 32 bits = fractional part of sqrt(p) * 2^32
        }
        let mut k = [0u32; 64];
        for (i, p) in ps.iter().enumerate() {
            k[i] = icbrt(p << 96) as u32;
        }
        (h0, k)
    })
}

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let k = &constants().1;
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes(block[4 * i..4 * i + 4].try_into().unwrap());
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(k[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        (h, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

fn sha256(data: &[u8]) -> Hash {
    let mut state = constants().0;
    // Padding: 0x80, zeros up to 56 mod 64, then the message length in BITS as big-endian u64.
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks_exact(64) {
        compress(&mut state, block);
    }
    let mut out = [0u8; 32];
    for (i, s) in state.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&s.to_be_bytes());
    }
    out
}

fn hex(h: &[u8]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------- Merkle tree

fn leaf_hash(data: &[u8]) -> Hash {
    let mut m = vec![0x00];
    m.extend_from_slice(data);
    sha256(&m)
}

fn node_hash(l: &Hash, r: &Hash) -> Hash {
    let mut m = Vec::with_capacity(65);
    m.push(0x01);
    m.extend_from_slice(l);
    m.extend_from_slice(r);
    sha256(&m)
}

/// Largest power of two strictly less than n (n >= 2).
fn split(n: usize) -> usize {
    n.next_power_of_two() / 2
}

struct TNode {
    hash: Hash,
    lo: usize,
    hi: usize,
    kids: Option<(usize, usize)>,
}

struct Tree {
    nodes: Vec<TNode>,
    n: usize,
}

/// Which side the sibling sits on, so the verifier knows the concatenation order.
#[derive(Clone, Debug)]
struct Step {
    sibling: Hash,
    sibling_is_left: bool,
}

impl Tree {
    fn build(leaves: &[Vec<u8>]) -> Tree {
        let hashes: Vec<Hash> = leaves.iter().map(|d| leaf_hash(d)).collect();
        let mut t = Tree {
            nodes: Vec::new(),
            n: leaves.len(),
        };
        if !hashes.is_empty() {
            t.build_range(&hashes, 0, hashes.len());
        }
        t
    }
    fn build_range(&mut self, hs: &[Hash], lo: usize, hi: usize) -> usize {
        let (hash, kids) = if hi - lo == 1 {
            (hs[lo], None)
        } else {
            let mid = lo + split(hi - lo);
            let l = self.build_range(hs, lo, mid);
            let r = self.build_range(hs, mid, hi);
            (
                node_hash(&self.nodes[l].hash, &self.nodes[r].hash),
                Some((l, r)),
            )
        };
        self.nodes.push(TNode { hash, lo, hi, kids });
        self.nodes.len() - 1
    }
    fn root_idx(&self) -> usize {
        self.nodes.len() - 1 // post-order construction: the root is pushed last
    }
    /// Root of the empty tree is H("") as in RFC 6962.
    fn root(&self) -> Hash {
        if self.n == 0 {
            sha256(b"")
        } else {
            self.nodes[self.root_idx()].hash
        }
    }
    fn prove(&self, i: usize) -> Vec<Step> {
        assert!(i < self.n);
        let mut steps = Vec::new();
        let mut cur = self.root_idx();
        while let Some((l, r)) = self.nodes[cur].kids {
            if i < self.nodes[r].lo {
                steps.push(Step {
                    sibling: self.nodes[r].hash,
                    sibling_is_left: false,
                });
                cur = l;
            } else {
                steps.push(Step {
                    sibling: self.nodes[l].hash,
                    sibling_is_left: true,
                });
                cur = r;
            }
        }
        steps.reverse(); // verifier folds from the leaf upwards
        steps
    }
}

fn verify(root: &Hash, leaf: &[u8], proof: &[Step]) -> bool {
    let mut h = leaf_hash(leaf);
    for s in proof {
        h = if s.sibling_is_left {
            node_hash(&s.sibling, &h)
        } else {
            node_hash(&h, &s.sibling)
        };
    }
    &h == root
}

// ---------------------------------------------------------------- append-only log (frontier)

/// Keeps only the roots of the perfect subtrees (one per set bit of n): O(log n) memory, O(1) amortised
/// append, exactly like the binary-counter carry. Old leaves never change, which is what a log needs.
#[derive(Default)]
struct Log {
    frontier: Vec<(Hash, usize)>, // (subtree root, leaf count), sizes strictly decreasing
}

impl Log {
    fn append(&mut self, data: &[u8]) {
        self.frontier.push((leaf_hash(data), 1));
        while self.frontier.len() >= 2 {
            let (r, rs) = self.frontier[self.frontier.len() - 1];
            let (l, ls) = self.frontier[self.frontier.len() - 2];
            if ls != rs {
                break;
            }
            self.frontier.truncate(self.frontier.len() - 2);
            self.frontier.push((node_hash(&l, &r), ls + rs)); // carry
        }
    }
    fn root(&self) -> Hash {
        let mut it = self.frontier.iter().rev();
        let Some(&(mut acc, _)) = it.next() else {
            return sha256(b"");
        };
        for (h, _) in it {
            acc = node_hash(h, &acc); // smaller right subtrees fold into the bigger left ones
        }
        acc
    }
}

// ---------------------------------------------------------------- anti-entropy

/// Returns (differing leaf indices, number of hash comparisons). Both trees must have the same leaf slots
/// (replicas of one key range); descending only where hashes differ prunes equal subtrees entirely.
fn diff(a: &Tree, b: &Tree) -> (Vec<usize>, usize) {
    assert_eq!(a.n, b.n);
    let mut out = Vec::new();
    let mut cmps = 0;
    if a.n > 0 {
        descend(a, b, a.root_idx(), b.root_idx(), &mut out, &mut cmps);
    }
    (out, cmps)
}

fn descend(a: &Tree, b: &Tree, ia: usize, ib: usize, out: &mut Vec<usize>, cmps: &mut usize) {
    *cmps += 1;
    if a.nodes[ia].hash == b.nodes[ib].hash {
        return;
    }
    match (a.nodes[ia].kids, b.nodes[ib].kids) {
        (Some((al, ar)), Some((bl, br))) => {
            descend(a, b, al, bl, out, cmps);
            descend(a, b, ar, br, out, cmps);
        }
        _ => out.push(a.nodes[ia].lo),
    }
}

// ---------------------------------------------------------------- demo / tests

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn rand_leaves(rng: &mut Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| (0..16).map(|_| rng.next() as u8).collect())
        .collect()
}

/// Prefix-free tree (no 0x00/0x01 tags): shows why domain separation matters.
fn naive_root(leaves: &[Vec<u8>]) -> Hash {
    if leaves.len() == 1 {
        return sha256(&leaves[0]);
    }
    let k = split(leaves.len());
    let mut m = naive_root(&leaves[..k]).to_vec();
    m.extend_from_slice(&naive_root(&leaves[k..]));
    sha256(&m)
}

fn naive_forgery() {
    let honest = vec![b"a".to_vec(), b"b".to_vec()];
    let root = naive_root(&honest);
    // Attacker claims a ONE-leaf tree whose single "leaf" is the concatenation of the two child hashes.
    let mut fake = sha256(b"a").to_vec();
    fake.extend_from_slice(&sha256(b"b"));
    assert_eq!(
        naive_root(&[fake.clone()]),
        root,
        "without domain separation the forgery collides"
    );
    // With the 0x00/0x01 prefixes the forged leaf hashes differently from the real inner node.
    let real = Tree::build(&honest).root();
    assert_ne!(Tree::build(&[fake]).root(), real);
    println!("second-preimage forgery: works on naive tree, fails with domain separation");
}

fn test_sha256() {
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let m448 = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"; // 448 bits: forces 2 blocks
    assert_eq!(
        hex(&sha256(m448)),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    let million = vec![b'a'; 1_000_000];
    assert_eq!(
        hex(&sha256(&million)),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
    println!("sha256 test vectors OK; abc = {}", hex(&sha256(b"abc")));
}

fn test_proofs(rng: &mut Rng) {
    for n in 1..=40usize {
        let leaves = rand_leaves(rng, n);
        let t = Tree::build(&leaves);
        let root = t.root();
        let depth = n.next_power_of_two().trailing_zeros() as usize;
        for (i, leaf) in leaves.iter().enumerate() {
            let p = t.prove(i);
            assert!(p.len() <= depth);
            assert!(verify(&root, leaf, &p), "n={n} i={i}");
            // Tamper: flip one bit of the leaf, of a sibling, or verify against another leaf.
            let mut bad = leaf.clone();
            bad[0] ^= 1;
            assert!(!verify(&root, &bad, &p));
            if let Some(first) = p.first() {
                let mut q = p.clone();
                q[0] = Step {
                    sibling: first.sibling,
                    sibling_is_left: !first.sibling_is_left,
                };
                assert!(!verify(&root, leaf, &q), "flipped direction must fail");
                q[0].sibling_is_left = first.sibling_is_left;
                q[0].sibling[3] ^= 0x10;
                assert!(!verify(&root, leaf, &q));
            }
            if n > 1 {
                assert!(!verify(&root, &leaves[(i + 1) % n], &p) || leaves[(i + 1) % n] == *leaf);
            }
        }
    }
    // Duplicate-last-leaf ambiguity (Bitcoin CVE-2012-2459 style) does not exist here.
    let abc: Vec<Vec<u8>> = vec![b"a".into(), b"b".into(), b"c".into()];
    let mut abcc = abc.clone();
    abcc.push(b"c".to_vec());
    assert_ne!(Tree::build(&abc).root(), Tree::build(&abcc).root());
    println!(
        "inclusion proofs verified for all leaves of trees of size 1..=40; tampering rejected"
    );
}

fn test_log(rng: &mut Rng) {
    let leaves = rand_leaves(rng, 200);
    let mut log = Log::default();
    assert_eq!(log.root(), Tree::build(&[]).root());
    for (i, d) in leaves.iter().enumerate() {
        log.append(d);
        assert_eq!(
            log.root(),
            Tree::build(&leaves[..=i]).root(),
            "size {}",
            i + 1
        );
        assert!(log.frontier.len() <= (i + 1).count_ones() as usize);
    }
    println!(
        "append-only log: incremental root == batch root for sizes 1..=200 (frontier = popcount(n) hashes)"
    );
}

fn test_anti_entropy(rng: &mut Rng) {
    for &n in &[1024usize, 1000, 777] {
        let a = rand_leaves(rng, n);
        let ta = Tree::build(&a);
        let depth = n.next_power_of_two().trailing_zeros() as usize;
        for trial in 0..40 {
            let k = trial % 6; // 0..=5 differing leaves
            let mut b = a.clone();
            let mut changed = Vec::new();
            while changed.len() < k {
                let i = rng.below(n);
                if !changed.contains(&i) {
                    b[i][0] ^= 0xff;
                    changed.push(i);
                }
            }
            changed.sort();
            let (found, cmps) = diff(&ta, &Tree::build(&b));
            let mut found_sorted = found.clone();
            found_sorted.sort();
            assert_eq!(found_sorted, changed, "n={n} trial={trial}");
            assert!(
                cmps <= 1 + 2 * k * (depth + 1),
                "{cmps} comparisons for k={k}"
            );
            if k == 0 {
                assert_eq!(cmps, 1, "identical replicas cost a single root comparison");
            }
        }
    }
    let a = rand_leaves(rng, 4096);
    let mut b = a.clone();
    for i in [17, 2000, 4095] {
        b[i][5] ^= 1;
    }
    let (found, cmps) = diff(&Tree::build(&a), &Tree::build(&b));
    println!(
        "anti-entropy: 4096 leaves, 3 differ -> found {found:?} with {cmps} hash comparisons (naive: 4096)"
    );
    assert!(cmps < 4096 / 20);
}

fn main() {
    test_sha256();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let t = Tree::build(&[b"x".to_vec(), b"y".to_vec(), b"z".to_vec()]);
    println!("root of [x,y,z] = {}", hex(&t.root()));
    naive_forgery();
    test_proofs(&mut rng);
    test_log(&mut rng);
    test_anti_entropy(&mut rng);
    println!("all Merkle checks passed");
}
