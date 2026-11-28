//! RECIPE 154 (integrity): SHA-256 from scratch + Merkle tree, inclusion proofs, append-only log, anti-entropy.
//! LEVEL: advanced
//! Use in: replica repair, tamper-evident logs, content-addressed storage.   Run: cargo run --release --example merkle_tree
#![allow(dead_code)]
//!
//! WHAT: a Merkle tree hashes leaves, then hashes pairs of hashes up to one root. Equal roots mean equal leaf
//! lists (up to hash collisions); a different root means "something differs".
//!
//! HOW (design choices, following the tree of RFC 6962 and RFC 9162, "Certificate Transparency"):
//!  - Domain separation: leaf = H(0x00 || data), node = H(0x01 || left || right). Without it, the 64 bytes
//!    `h(a)||h(b)` could be presented as a *leaf* and hash to the same value as the inner node (second-preimage
//!    attack on the tree). `naive_forgery` below demonstrates this on a tree with no prefixes.
//!  - Odd node count: NO duplication of the last node. Some designs pair a lone node with itself. Then [a,b,c]
//!    and [a,b,c,c] share a root unless the leaf count is trusted too (`duplicate_pitfall`; Bitcoin's
//!    CVE-2012-2459). This tree follows another rule: split n leaves at the largest power of
//!    two k < n: MTH(D) = H(1 || MTH(D[..k]) || MTH(D[k..])). A lone node moves up unchanged. The incremental
//!    log and the batch tree agree (tested).
//!  - Inclusion proof = sibling hashes bottom-up. The verifier takes the leaf's index and the tree size, and works
//!    out each side itself (RFC 9162, 2.1.3.2). The root and the size must come TOGETHER from a source you trust:
//!    the root alone does not pin the size, and a proof can verify under a size that is not the real one.
//!  - Anti-entropy: two replicas with the same leaf slots compare roots, then descend only into children whose
//!    hashes differ: cost ~ O(k log n) hash comparisons for k differing leaves, instead of n.
//!
//! NOT HERE: consistency proofs (that a newer root extends an older one), signatures on roots, and any way to
//! learn whether everyone was shown the same root. SHA-256 is hand-written for teaching.
//!
//! COMMON USES: Git and Mercurial-style content-addressed object graphs (hash-linked trees), Bitcoin block
//! headers committing to transactions (SPV proofs), Certificate Transparency logs (RFC 6962), Dynamo-style
//! anti-entropy and Cassandra's repair, which compare Merkle trees of key ranges to find divergent replicas,
//! and content-addressed stores/packages that verify chunks against a trusted root.
//! Read: https://www.rfc-editor.org/rfc/rfc9162 (section 2.1), https://www.rfc-editor.org/rfc/rfc6962
//! Read: https://csrc.nist.gov/pubs/fips/180-4/upd1/final (SHA-256 spec)
//! Read: https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf
use std::sync::OnceLock;

// ---------------------------------------------------------------- SHA-256 (FIPS 180-4)

type Hash = [u8; 32];

fn primes(count: usize) -> Vec<u128> {
    let mut v: Vec<u128> = Vec::new();
    let mut c = 2u128;
    while v.len() < count {
        if v.iter().all(|p| !c.is_multiple_of(*p)) {
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
    for block in msg.as_chunks::<64>().0 {
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
    /// The sibling hashes from leaf `i` up to the root. No sides: the verifier derives them.
    fn prove(&self, i: usize) -> Vec<Hash> {
        assert!(i < self.n);
        let mut path = Vec::new();
        let mut cur = self.root_idx();
        while let Some((l, r)) = self.nodes[cur].kids {
            let (next, sibling) = if i < self.nodes[r].lo { (l, r) } else { (r, l) };
            path.push(self.nodes[sibling].hash);
            cur = next;
        }
        path.reverse(); // verifier folds from the leaf upwards
        path
    }
}

/// RFC 9162, section 2.1.3.2. `root` and `size` are what the verifier already trusts. `index`, `leaf` and
/// `path` may come from anyone.
fn verify_inclusion(root: &Hash, size: u64, index: u64, leaf: &[u8], path: &[Hash]) -> bool {
    if index >= size {
        return false;
    }
    let (mut node, mut last) = (index, size - 1);
    let mut hash = leaf_hash(leaf);
    for sibling in path {
        if last == 0 {
            return false; // the path is too long
        }
        if node & 1 == 1 || node == last {
            hash = node_hash(sibling, &hash);
            // the last node of a level may move up several levels with no sibling
            while node & 1 == 0 && node != 0 {
                node >>= 1;
                last >>= 1;
            }
        } else {
            hash = node_hash(&hash, sibling);
        }
        node >>= 1;
        last >>= 1;
    }
    // `last == 0` means the path reached the top: a path that is too short fails here
    last == 0 && hash == *root
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
    fn hash(&mut self) -> Hash {
        std::array::from_fn(|_| self.next() as u8)
    }
}

/// The definition in RFC 9162, written as it reads. Slow and obvious: the oracle for `Tree`.
fn oracle_root(d: &[Vec<u8>]) -> Hash {
    match d.len() {
        0 => sha256(b""),
        1 => leaf_hash(&d[0]),
        n => {
            let k = split(n);
            node_hash(&oracle_root(&d[..k]), &oracle_root(&d[k..]))
        }
    }
}

fn rand_leaves(rng: &mut Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| (0..16).map(|_| rng.next() as u8).collect())
        .collect()
}

/// A tree with no 0x00/0x01 prefixes: shows why domain separation matters.
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
    for n in 0..=70usize {
        let leaves = rand_leaves(rng, n);
        let t = Tree::build(&leaves);
        let root = t.root();
        assert_eq!(root, oracle_root(&leaves), "n={n}");
        let size = n as u64;
        let depth = n.max(1).next_power_of_two().trailing_zeros() as usize;
        for (i, leaf) in leaves.iter().enumerate() {
            let p = t.prove(i);
            let index = i as u64;
            assert!(p.len() <= depth);
            assert!(
                verify_inclusion(&root, size, index, leaf, &p),
                "n={n} i={i}"
            );
            // another leaf, another position, another root
            let mut bad = leaf.clone();
            bad[0] ^= 1;
            assert!(!verify_inclusion(&root, size, index, &bad, &p));
            for other in (0..size + 3).filter(|o| *o != index) {
                assert!(
                    !verify_inclusion(&root, size, other, leaf, &p),
                    "n={n} i={i} as {other}"
                );
            }
            let mut wrong_root = root;
            wrong_root[0] ^= 1;
            assert!(!verify_inclusion(&wrong_root, size, index, leaf, &p));
            // a path that is changed, too short or too long
            for at in 0..p.len() {
                let mut q = p.clone();
                q[at][3] ^= 0x10;
                assert!(!verify_inclusion(&root, size, index, leaf, &q));
            }
            if !p.is_empty() {
                assert!(!verify_inclusion(
                    &root,
                    size,
                    index,
                    leaf,
                    &p[..p.len() - 1]
                ));
            }
            let mut long = p.clone();
            long.push(rng.hash());
            assert!(!verify_inclusion(&root, size, index, leaf, &long));
            assert!(!verify_inclusion(&root, 0, index, leaf, &p));
        }
    }
    // leaf 1 of a 2-leaf tree, passed off as the only leaf of a 1-leaf tree with one step too many
    let ab = Tree::build(&[b"a".into(), b"b".into()]);
    assert!(!verify_inclusion(&ab.root(), 1, 0, b"b", &ab.prove(1)));
    // hostile numbers and random paths: no panic, nothing accepted
    let root = Tree::build(&rand_leaves(rng, 9)).root();
    for round in 0..3_000 {
        let size = [1, 2, 9, 1 << 63, u64::MAX, rng.next()][round % 6];
        let index = [
            0,
            size - u64::from(size > 0),
            u64::MAX,
            rng.next() % size.max(1),
        ][round % 4];
        let len = rng.below(200);
        let path: Vec<Hash> = (0..len).map(|_| rng.hash()).collect();
        assert!(!verify_inclusion(&root, size, index, b"x", &path));
    }
    println!(
        "roots match the definition, sizes 0..=70; every honest proof verifies; changes are rejected"
    );
}

/// The root does not pin the size: so the size must be trusted too.
fn size_is_not_in_the_root() {
    let abc: Vec<Vec<u8>> = vec![b"a".into(), b"b".into(), b"c".into()];
    let t = Tree::build(&abc);
    let path = t.prove(0);
    assert!(verify_inclusion(&t.root(), 3, 0, b"a", &path));
    assert!(
        verify_inclusion(&t.root(), 4, 0, b"a", &path),
        "a false size gets through"
    );
    println!("a proof from a 3-leaf tree also passes with the claimed size 4");

    // a false size can carry a false POSITION too: the last of 8 leaves passes as position 14 of 15
    let eight: Vec<Vec<u8>> = (0..8u8).map(|i| vec![i]).collect();
    let t = Tree::build(&eight);
    let path = t.prove(7);
    assert!(verify_inclusion(&t.root(), 8, 7, &eight[7], &path));
    assert!(verify_inclusion(&t.root(), 15, 14, &eight[7], &path));
    assert!(!verify_inclusion(&t.root(), 8, 14, &eight[7], &path));
    println!("with a false size 15, the leaf at position 7 passes as position 14");
}

/// What pairing a lone node with itself does to three leaves, written out by hand.
fn duplicate_pitfall() {
    let [a, b, c] = [b"a", b"b", b"c"].map(|x| leaf_hash(x));
    let ab = node_hash(&a, &b);
    // three leaves: c has no partner, so such a design pairs it with itself
    let three = node_hash(&ab, &node_hash(&c, &c));
    // four leaves, the last one really repeated: an ordinary full tree
    let abcc: Vec<Vec<u8>> = vec![b"a".into(), b"b".into(), b"c".into(), b"c".into()];
    assert_eq!(three, Tree::build(&abcc).root());
    // this file's tree keeps the two lists apart
    assert_ne!(Tree::build(&abcc[..3]).root(), Tree::build(&abcc).root());
    println!("pairing a lone node with itself: [a,b,c] and [a,b,c,c] share a root");
}

/// Compare roots and proofs with the same two functions written in Python from the RFC, using `hashlib`:
/// sizes 0..=64, random leaves of 0..80 bytes from the xorshift generator below.
fn check_against_python() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut all = Vec::new();
    for n in 0..=64 {
        let leaves: Vec<Vec<u8>> = (0..n)
            .map(|_| {
                let len = rng.below(80);
                (0..len).map(|_| (rng.next() >> 24) as u8).collect()
            })
            .collect();
        let t = Tree::build(&leaves);
        all.extend(t.root());
        for i in 0..n {
            all.extend(t.prove(i).into_iter().flatten());
        }
    }
    assert_eq!(
        hex(&sha256(&all)),
        "85d3b2adef9d247770263c54a13f52b1ab819a5795dcc7ccfa82c808d3eb7e0d"
    );
}

/// What a saved checkpoint (root and size) catches, and what it does not.
fn test_tamper_evidence(rng: &mut Rng) {
    let records = rand_leaves(rng, 20);
    let (saved_root, saved_size) = (Tree::build(&records[..12]).root(), 12);
    // true only if the first `saved_size` records are the ones the checkpoint was made from
    let still_matches = |log: &[Vec<u8>]| {
        log.len() >= saved_size && Tree::build(&log[..saved_size]).root() == saved_root
    };
    assert!(still_matches(&records));

    let mut changed = records.clone();
    changed[4][0] ^= 1;
    assert!(!still_matches(&changed));
    let mut removed = records.clone();
    removed.remove(4);
    assert!(!still_matches(&removed));
    assert!(!still_matches(&records[..11]));

    // whoever rewrites the log can also make a new root and proofs that agree with it
    let forged = Tree::build(&changed[..12]);
    assert!(verify_inclusion(
        &forged.root(),
        12,
        4,
        &changed[4],
        &forged.prove(4)
    ));
    assert!(!verify_inclusion(
        &saved_root,
        12,
        4,
        &changed[4],
        &forged.prove(4)
    ));

    // and nothing after the checkpoint is protected by it
    let mut later = records.clone();
    later[15][0] ^= 1;
    assert!(still_matches(&later) && still_matches(&records[..12]));
    println!("a saved root and size catch changes before the checkpoint, and none after it");
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
    duplicate_pitfall();
    check_against_python();
    test_proofs(&mut rng);
    size_is_not_in_the_root();
    test_tamper_evidence(&mut rng);
    test_log(&mut rng);
    test_anti_entropy(&mut rng);
    println!("all Merkle checks passed");
}
